//! # Property-based tests — Vero Consensus Invariants
//!
//! Randomized, property-based verification of the pure consensus logic in
//! `src/consensus.rs` (plan.md → "Property-based tests").
//!
//! These tests complement the hand-written unit tests in `tests/consensus.rs`
//! and the exhaustive Kani harnesses in `verification/`. Where unit tests pin
//! down known edge cases, the properties below sweep randomized sequences of
//! votes across the entire `u64` weight space looking for invariants that no
//! one thought to hand-encode:
//!
//! 1. **Vote/weight accumulation never overflows** — `votes` saturates at
//!    `u32::MAX` (never wraps) and `total_weight_accrued` is accumulated with
//!    `checked_add`, so a sequence of votes can never silently wrap `u64`. On
//!    a would-be overflow the call errors and the state is left untouched.
//! 2. **`is_done` is monotonic** — once `is_done` becomes `true`, no sequence
//!    of further operations can ever set it back to `false`.
//!
//! These are the security-relevant guarantees the Wave Program prioritised
//! for property coverage (see plan.md, "Integer overflow guards" and
//! "Property-based tests").

use proptest::prelude::*;
use proptest::test_runner::Config;

use vero_core_contracts::consensus::{
    apply_vote, resolution_invariant_holds, ConsensusError, ConsensusState,
};

proptest! {
    #![proptest_config(Config::with_cases(1024))]

    // ─── Invariant 1: votes / weight accumulation never overflow ────────────

    /// Across a randomized sequence of `u64` weights, the vote counter must
    /// track the number of successfully applied votes (never wrapping past
    /// `u32::MAX`), and `total_weight_accrued` must equal the `checked_add`
    /// accumulation of every successful (non-overflowing, non-zero) vote.
    #[test]
    fn prop_accumulation_never_overflows(
        weights in prop::collection::vec(any::<u64>(), 0..128),
        threshold in any::<u64>(),
    ) {
        let mut state = ConsensusState::new();
        let mut expected_total: u64 = 0;
        let mut ok_votes: u32 = 0;

        for &weight in &weights {
            if weight == 0 {
                // Zero-weight votes are rejected and must leave state untouched.
                let before = state;
                assert_eq!(
                    apply_vote(&mut state, weight, threshold),
                    Err(ConsensusError::ZeroWeight)
                );
                assert_eq!(state, before);
                continue;
            }

            match expected_total.checked_add(weight) {
                Some(sum) => {
                    // Fits: the accumulation proceeds and is reflected exactly.
                    expected_total = sum;
                    ok_votes += 1;
                    apply_vote(&mut state, weight, threshold).unwrap();
                }
                None => {
                    // Would overflow: apply_vote must error and not partially
                    // mutate the state (checked_add prevents silent wraparound).
                    let before = state;
                    assert_eq!(
                        apply_vote(&mut state, weight, threshold),
                        Err(ConsensusError::WeightOverflow)
                    );
                    assert_eq!(state, before);
                }
            }

            // `votes` is a saturating counter — it can never exceed u32::MAX.
            assert_eq!(state.votes, ok_votes);
            // `total_weight_accrued` uses checked_add — it can never wrap.
            assert_eq!(state.total_weight_accrued, expected_total);
        }
    }

    /// Drive the vote counter and weight accumulator right up against their
    /// type maxima and keep voting. The counter must saturate at `u32::MAX`
    /// (never overflow), and every weight that would push `total_weight_accrued`
    /// past `u64::MAX` must be rejected cleanly with state left unchanged.
    #[test]
    fn prop_saturation_at_max_limits(
        seed_votes in (u32::MAX - 4)..=u32::MAX,
        seed_weight in (u64::MAX - 4)..=u64::MAX,
        weights in prop::collection::vec(1u64..=8, 0..32),
    ) {
        let mut state = ConsensusState {
            total_weight_accrued: seed_weight,
            votes: seed_votes,
            is_done: false,
        };
        let mut expected_total = seed_weight;
        let mut ok_votes: u32 = 0;

        for &weight in &weights {
            match expected_total.checked_add(weight) {
                Some(sum) => {
                    expected_total = sum;
                    ok_votes = ok_votes.saturating_add(1);
                    apply_vote(&mut state, weight, u64::MAX).unwrap();
                }
                None => {
                    let before = state;
                    assert_eq!(
                        apply_vote(&mut state, weight, u64::MAX),
                        Err(ConsensusError::WeightOverflow)
                    );
                    assert_eq!(state, before);
                }
            }

            // With a seed near u32::MAX, the saturating counter pins at
            // u32::MAX (never exceeding the type maximum) rather than wrapping
            // around to 0.
            assert_eq!(state.votes, seed_votes.saturating_add(ok_votes));
            assert_eq!(state.total_weight_accrued, expected_total);
        }
    }

    // ─── Invariant 2: is_done is monotonic ───────────────────────────────────

    /// Once `is_done` is set to `true`, no continuation of the vote sequence
    /// can ever set it back to `false` — regardless of subsequent weights,
    /// zero-weight rejections, or overflow errors.
    #[test]
    fn prop_is_done_monotonic(
        weights in prop::collection::vec(any::<u64>(), 0..128),
        threshold in any::<u64>(),
    ) {
        let mut state = ConsensusState::new();
        let mut ever_done = false;

        for &weight in &weights {
            // Rejections (zero weight / overflow) must not perturb is_done.
            let _ = apply_vote(&mut state, weight, threshold);

            if ever_done {
                assert!(
                    state.is_done,
                    "is_done was cleared after being set to true"
                );
            }
            ever_done = ever_done || state.is_done;
        }

        // The final state must agree with what we observed along the way, and
        // the resolution invariant ("is_done ⇒ weight ≥ threshold") still holds.
        assert_eq!(state.is_done, ever_done);
        assert!(resolution_invariant_holds(&state, threshold));
    }

    /// Monotonicity under a reachable threshold: with enough positive votes
    /// the task is guaranteed to resolve, and from that moment on `is_done`
    /// stays `true` through the remainder of the (longer) vote sequence.
    #[test]
    fn prop_is_done_monotonic_under_repeated_votes(
        weights in prop::collection::vec(1u64..=10, 50..128),
        threshold in 0u64..50,
    ) {
        let mut state = ConsensusState::new();
        let mut ever_done = false;

        for &weight in &weights {
            let _ = apply_vote(&mut state, weight, threshold);

            if ever_done {
                assert!(
                    state.is_done,
                    "is_done was cleared after being set to true"
                );
            }
            ever_done = ever_done || state.is_done;
        }

        // Each weight is ≥ 1 and there are ≥ 50 of them, so total_weight_accrued
        // reaches ≥ 50 > 49 ≥ threshold — resolution is unavoidable here.
        assert!(ever_done, "reachable-threshold sequence never resolved");
        assert!(state.is_done);
    }

    /// Any threshold reachable via `set_weight_threshold` (i.e. passing
    /// `validate_weight_threshold`) is unconditionally accepted by
    /// `migrate::validate_migration`.
    #[test]
    fn prop_reachable_threshold_accepted_by_migration(
        threshold in 1u64..=vero_core_contracts::limits::MAX_WEIGHT_THRESHOLD,
    ) {
        let env = soroban_sdk::Env::default();
        let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);

        env.as_contract(&contract_id, || {
            let mut cache = vero_core_contracts::migrate::MigrationCache::new(&env);

            // Pre-requisite for migration pre-flight: valid storage version
            cache.set(
                &vero_core_contracts::DataKey::StorageVersion,
                &vero_core_contracts::migrate::CURRENT_VERSION,
            );
            cache.set(&vero_core_contracts::DataKey::WeightThreshold, &threshold);

            // Threshold validator directly accepts
            assert_eq!(
                vero_core_contracts::validation::validate_weight_threshold(threshold),
                Ok(())
            );

            // Migration pre-flight check must accept the same threshold
            assert_eq!(
                vero_core_contracts::migrate::validate_migration(&env, &cache),
                Ok(())
            );
        });
    }

    /// Any threshold rejected by `set_weight_threshold` (0 or > MAX_WEIGHT_THRESHOLD)
    /// is rejected with the exact same error code by `migrate::validate_migration`.
    #[test]
    fn prop_invalid_threshold_rejected_identically_by_migration(
        threshold in proptest::prop_oneof![
            Just(0u64),
            (vero_core_contracts::limits::MAX_WEIGHT_THRESHOLD + 1)..=u64::MAX,
        ]
    ) {
        let env = soroban_sdk::Env::default();
        let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);

        env.as_contract(&contract_id, || {
            let mut cache = vero_core_contracts::migrate::MigrationCache::new(&env);

            cache.set(
                &vero_core_contracts::DataKey::StorageVersion,
                &vero_core_contracts::migrate::CURRENT_VERSION,
            );
            cache.set(&vero_core_contracts::DataKey::WeightThreshold, &threshold);

            let direct_err =
                vero_core_contracts::validation::validate_weight_threshold(threshold).unwrap_err();
            let migration_err =
                vero_core_contracts::migrate::validate_migration(&env, &cache).unwrap_err();

            assert_eq!(direct_err, migration_err);
            if threshold == 0 {
                assert_eq!(
                    direct_err,
                    vero_core_contracts::ContractError::InvalidAmount
                );
            } else {
                assert_eq!(
                    direct_err,
                    vero_core_contracts::ContractError::InvalidRange
                );
            }
        });
    }
}
