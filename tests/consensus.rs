//! Unit tests for the pure consensus logic (`src/consensus.rs`).
//!
//! These tests were previously inline in `src/consensus.rs` as a
//! `#[cfg(test)] mod tests` block. They have been moved here to follow the
//! crate's convention of keeping all tests under `tests/`.
//!
//! ## Why this module is Env-free
//!
//! `consensus.rs` is deliberately kept free of Soroban `Env` types so that
//! Kani (and other model checkers) can formally verify its logic without
//! mocking the Soroban host environment. The tests here reflect that same
//! design: they exercise only pure Rust structs and functions and do **not**
//! depend on `soroban-sdk`'s `testutils` feature.
//!
//! The Kani proof harnesses in `verification/` and the safety-invariant checks
//! in `tests/safety_invariants.rs` provide complementary coverage from
//! different angles (exhaustive symbolic verification and runtime invariant
//! checks, respectively).

use vero_core_contracts::consensus::{
    apply_vote, resolution_invariant_holds, ConsensusError, ConsensusState,
};

#[test]
fn test_apply_vote_resolves_at_threshold() {
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 300, 300).unwrap();
    assert!(state.is_done);
    assert_eq!(state.total_weight_accrued, 300);
    assert_eq!(state.votes, 1);
}

#[test]
fn test_apply_vote_does_not_resolve_below_threshold() {
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 299, 300).unwrap();
    assert!(!state.is_done);
    assert_eq!(state.total_weight_accrued, 299);
}

#[test]
fn test_apply_vote_resolves_above_threshold() {
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 500, 300).unwrap();
    assert!(state.is_done);
    assert_eq!(state.total_weight_accrued, 500);
}

#[test]
fn test_apply_vote_accumulates_across_multiple_votes() {
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 100, 300).unwrap();
    assert!(!state.is_done);
    apply_vote(&mut state, 100, 300).unwrap();
    assert!(!state.is_done);
    apply_vote(&mut state, 100, 300).unwrap();
    assert!(state.is_done);
    assert_eq!(state.total_weight_accrued, 300);
    assert_eq!(state.votes, 3);
}

#[test]
fn test_apply_vote_rejects_zero_weight() {
    let mut state = ConsensusState::new();
    let err = apply_vote(&mut state, 0, 300).unwrap_err();
    assert_eq!(err, ConsensusError::ZeroWeight);
    assert!(!state.is_done);
    assert_eq!(state.total_weight_accrued, 0);
}

#[test]
fn test_apply_vote_overflow_protection() {
    let mut state = ConsensusState::new();
    state.total_weight_accrued = u64::MAX;
    let err = apply_vote(&mut state, 1, 300).unwrap_err();
    assert_eq!(err, ConsensusError::WeightOverflow);
    // State must be unchanged after error
    assert_eq!(state.total_weight_accrued, u64::MAX);
    assert!(!state.is_done);
}

#[test]
fn test_votes_counter_saturates() {
    let mut state = ConsensusState::new();
    state.votes = u32::MAX;
    // Should saturate, not overflow
    apply_vote(&mut state, 1, u64::MAX).unwrap();
    assert_eq!(state.votes, u32::MAX);
}

#[test]
fn test_is_done_monotone_once_set() {
    // Once is_done is true, subsequent votes keep it true
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 300, 300).unwrap();
    assert!(state.is_done);
    // Simulate more votes after resolution — is_done stays true
    apply_vote(&mut state, 100, 300).unwrap();
    assert!(state.is_done);
}

#[test]
fn test_zero_threshold_first_vote_resolves() {
    // Threshold = 0: any non-zero weight vote immediately resolves.
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 1, 0).unwrap();
    assert!(state.is_done);
}

#[test]
fn test_resolution_invariant_holds_after_vote() {
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 400, 300).unwrap();
    assert!(resolution_invariant_holds(&state, 300));
}

#[test]
fn test_resolution_invariant_holds_before_threshold() {
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 200, 300).unwrap();
    assert!(resolution_invariant_holds(&state, 300));
    assert!(!state.is_done);
}

#[test]
fn test_consensus_state_default_is_new() {
    // ConsensusState::default() and ConsensusState::new() must be identical
    let via_new = ConsensusState::new();
    let via_default = ConsensusState::default();
    assert_eq!(via_new, via_default);
    assert_eq!(via_new.total_weight_accrued, 0);
    assert_eq!(via_new.votes, 0);
    assert!(!via_new.is_done);
}

#[test]
fn test_consensus_resolves_at_maximum_allowed_threshold() {
    use vero_core_contracts::limits::MAX_WEIGHT_THRESHOLD;

    let mut state = ConsensusState::new();
    apply_vote(&mut state, MAX_WEIGHT_THRESHOLD, MAX_WEIGHT_THRESHOLD).unwrap();
    assert!(state.is_done);
    assert_eq!(state.total_weight_accrued, MAX_WEIGHT_THRESHOLD);
    assert!(resolution_invariant_holds(&state, MAX_WEIGHT_THRESHOLD));
}

#[test]
fn test_consensus_resolves_at_minimum_valid_threshold() {
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 1, 1).unwrap();
    assert!(state.is_done);
    assert_eq!(state.total_weight_accrued, 1);
    assert!(resolution_invariant_holds(&state, 1));
}
