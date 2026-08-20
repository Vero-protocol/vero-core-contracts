//! # Safety Invariant Tests — Vero Consensus Logic
//!
//! These tests verify that the core consensus logic satisfies its safety
//! invariants at runtime. They mirror the Kani proof harnesses and K-framework
//! specifications, providing a concrete execution check that runs as part of
//! `cargo test`.
//!
//! The invariants tested here correspond to the formal model in `proofs/`:
//!
//! | Test | Invariant | Kani Harness | K Claim |
//! |------|-----------|-------------|---------|
//! | `invariant_threshold` | is_done ⇒ weight ≥ threshold | `proof_threshold_invariant` | P1 |
//! | `invariant_no_below_threshold` | No path sets is_done below threshold | `proof_no_below_threshold_resolution` | P2 |
//! | `invariant_monotone_done` | is_done never cleared | `proof_monotone_done` | P3 |
//! | `invariant_zero_weight_rejected` | weight=0 returns error | `proof_zero_threshold_safe` | P4 |
//! | `invariant_overflow_caught` | checked_add catches overflow | `proof_weight_overflow_impossible` | P5 |
//! | `invariant_votes_saturate` | votes counter saturates | `proof_votes_no_overflow` | — |
//! | `invariant_multi_vote_accumulation` | weights sum correctly | `proof_multi_vote_accumulation` | — |

use vero_core_contracts::consensus::{
    apply_vote, resolution_invariant_holds, ConsensusError, ConsensusState,
};

// ─── Helper ───────────────────────────────────────────────────────────────────

/// Asserts that the threshold invariant holds for a given state.
fn assert_threshold_invariant(state: &ConsensusState, threshold: u64) {
    assert!(
        resolution_invariant_holds(state, threshold),
        "INVARIANT VIOLATION: is_done={}, total_weight_accrued={}, threshold={}",
        state.is_done,
        state.total_weight_accrued,
        threshold
    );
}

// ─── I1: Threshold Invariant ──────────────────────────────────────────────────

#[test]
fn invariant_threshold_met_resolves() {
    // When weight >= threshold, is_done must be true
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 300, 300).unwrap();
    assert!(state.is_done);
    assert_threshold_invariant(&state, 300);
}

#[test]
fn invariant_threshold_not_met_does_not_resolve() {
    // When weight < threshold, is_done must be false
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 200, 300).unwrap();
    assert!(!state.is_done);
    assert_threshold_invariant(&state, 300);
}

#[test]
fn invariant_threshold_exactly_at_boundary() {
    // Edge case: weight == threshold exactly
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 0, 0).unwrap_err(); // zero weight rejected
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 1, 1).unwrap();
    assert!(state.is_done);
    assert_threshold_invariant(&state, 1);
}

// ─── I2: No Below-Threshold Resolution ────────────────────────────────────────

#[test]
fn invariant_no_below_threshold_resolution() {
    // Exhaustively test that is_done is never set when total < threshold
    // Test various starting states and weights
    let thresholds = [0u64, 1, 100, 300, 1000, u64::MAX];
    let initial_weights = [0u64, 50, 200, 500, u64::MAX - 1];
    let vote_weights = [1u64, 50, 100, 200, u64::MAX];

    for &threshold in &thresholds {
        for &initial in &initial_weights {
            for &weight in &vote_weights {
                // Skip cases that would overflow
                if initial.checked_add(weight).is_none() {
                    continue;
                }
                // Skip zero weight (always rejected)
                if weight == 0 {
                    continue;
                }

                let mut state = ConsensusState {
                    total_weight_accrued: initial,
                    votes: 0,
                    is_done: false,
                };

                let _ = apply_vote(&mut state, weight, threshold);

                // Core invariant: if is_done is true, total must be >= threshold
                if state.is_done {
                    assert!(
                        state.total_weight_accrued >= threshold,
                        "SECURITY VIOLATION: is_done=true but total_weight_accrued={} < threshold={} (initial={}, weight={})",
                        state.total_weight_accrued,
                        threshold,
                        initial,
                        weight
                    );
                }
            }
        }
    }
}

// ─── I3: Monotonic isDone ─────────────────────────────────────────────────────

#[test]
fn invariant_monotone_done() {
    // Once is_done is true, it must never become false
    let mut state = ConsensusState {
        total_weight_accrued: 500,
        votes: 5,
        is_done: true, // already resolved
    };

    // Subsequent votes must keep is_done = true
    for &weight in &[1u64, 100, u64::MAX] {
        if state.total_weight_accrued.checked_add(weight).is_some() {
            let _ = apply_vote(&mut state, weight, 300);
            assert!(
                state.is_done,
                "INVARIANT VIOLATION: is_done was cleared after being set"
            );
        }
    }
}

// ─── I4: Zero Weight Rejected ─────────────────────────────────────────────────

#[test]
fn invariant_zero_weight_rejected() {
    let mut state = ConsensusState::new();
    let result = apply_vote(&mut state, 0, 300);
    assert_eq!(result, Err(ConsensusError::ZeroWeight));
    // State must be unchanged
    assert_eq!(state.total_weight_accrued, 0);
    assert_eq!(state.votes, 0);
    assert!(!state.is_done);
}

// ─── I5: Overflow Caught ──────────────────────────────────────────────────────

#[test]
fn invariant_overflow_caught() {
    let mut state = ConsensusState {
        total_weight_accrued: u64::MAX,
        votes: 0,
        is_done: false,
    };
    let before = state.total_weight_accrued;

    let result = apply_vote(&mut state, 1, 300);
    assert_eq!(result, Err(ConsensusError::WeightOverflow));
    // State must be unchanged on overflow
    assert_eq!(state.total_weight_accrued, before);
    assert!(!state.is_done);
}

// ─── I6: Votes Counter Saturates ──────────────────────────────────────────────

#[test]
fn invariant_votes_saturate() {
    let mut state = ConsensusState {
        total_weight_accrued: 0,
        votes: u32::MAX,
        is_done: false,
    };

    // Should saturate, not overflow
    apply_vote(&mut state, 1, u64::MAX).unwrap();
    assert_eq!(state.votes, u32::MAX);
}

// ─── I7: Multi-Vote Accumulation ──────────────────────────────────────────────

#[test]
fn invariant_multi_vote_accumulation() {
    let mut state = ConsensusState::new();
    let threshold = 300u64;

    // Vote 1: weight 100
    apply_vote(&mut state, 100, threshold).unwrap();
    assert!(!state.is_done);
    assert_eq!(state.total_weight_accrued, 100);
    assert_eq!(state.votes, 1);

    // Vote 2: weight 100
    apply_vote(&mut state, 100, threshold).unwrap();
    assert!(!state.is_done);
    assert_eq!(state.total_weight_accrued, 200);
    assert_eq!(state.votes, 2);

    // Vote 3: weight 100 — reaches threshold
    apply_vote(&mut state, 100, threshold).unwrap();
    assert!(state.is_done);
    assert_eq!(state.total_weight_accrued, 300);
    assert_eq!(state.votes, 3);

    // Vote 4: weight 100 — beyond threshold, is_done stays true
    apply_vote(&mut state, 100, threshold).unwrap();
    assert!(state.is_done);
    assert_eq!(state.total_weight_accrued, 400);
    assert_eq!(state.votes, 4);
}

// ─── I8: Resolution Invariant Helper ──────────────────────────────────────────

#[test]
fn invariant_resolution_helper_soundness() {
    // The resolution_invariant_holds() helper must correctly identify safe states
    let mut state = ConsensusState::new();

    // Before any votes: not done, invariant holds
    assert!(resolution_invariant_holds(&state, 300));

    // After vote below threshold: not done, invariant holds
    apply_vote(&mut state, 200, 300).unwrap();
    assert!(resolution_invariant_holds(&state, 300));

    // After vote meeting threshold: done, invariant holds
    apply_vote(&mut state, 100, 300).unwrap();
    assert!(resolution_invariant_holds(&state, 300));

    // After vote exceeding threshold: done, invariant holds
    apply_vote(&mut state, 500, 300).unwrap();
    assert!(resolution_invariant_holds(&state, 300));
}

// ─── I9: Zero Threshold Edge Case ─────────────────────────────────────────────

#[test]
fn invariant_zero_threshold_safe() {
    // threshold = 0: any non-zero weight vote must resolve immediately
    let mut state = ConsensusState::new();
    apply_vote(&mut state, 1, 0).unwrap();
    assert!(state.is_done);
    assert_eq!(state.total_weight_accrued, 1);
    assert_threshold_invariant(&state, 0);
}

// ─── I10: Maximum Weight Guardian ─────────────────────────────────────────────

#[test]
fn invariant_max_weight_single_guardian() {
    // A guardian with u64::MAX weight must resolve any task
    let mut state = ConsensusState::new();
    apply_vote(&mut state, u64::MAX, 0).unwrap();
    assert!(state.is_done);
    assert_eq!(state.total_weight_accrued, u64::MAX);

    // Must also work with any threshold
    let mut state = ConsensusState::new();
    apply_vote(&mut state, u64::MAX, u64::MAX).unwrap();
    assert!(state.is_done);
    assert_eq!(state.total_weight_accrued, u64::MAX);
}
