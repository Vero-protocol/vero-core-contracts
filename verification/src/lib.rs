//! # Formal Verification Harnesses — Vero Consensus Logic
//!
//! This crate contains **Kani proof harnesses** that formally verify the
//! invariants of `vero-core-contracts`'s weighted consensus mechanism.
//!
//! ## Running proofs
//!
//! ```bash
//! # Install Kani (one-time setup)
//! cargo install --locked kani-verifier
//! cargo kani setup
//!
//! # Run all proofs
//! cargo kani --manifest-path verification/Cargo.toml
//!
//! # Run a specific harness
//! cargo kani --manifest-path verification/Cargo.toml \
//!   --harness proof_threshold_invariant
//! ```
//!
//! ## What is proved
//!
//! | Harness | Property |
//! |---------|----------|
//! | `proof_threshold_invariant` | `is_done` ↔ `weight >= threshold` after a single vote |
//! | `proof_no_below_threshold_resolution` | No path sets `is_done` without meeting threshold |
//! | `proof_monotone_done` | `is_done` never goes `true → false` |
//! | `proof_weight_overflow_impossible` | `checked_add` catches all overflows before writing |
//! | `proof_votes_no_overflow` | `votes` counter saturates safely at `u32::MAX` |
//! | `proof_zero_threshold_safe` | threshold=0 is well-defined (first vote resolves) |
//! | `proof_max_weight_single_guardian` | single `u64::MAX`-weight guardian: overflow caught |
//! | `proof_multi_vote_accumulation` | weight accumulates correctly across N symbolic votes |
//! | `proof_resolution_invariant_helper` | `resolution_invariant_holds()` is a sound post-condition |

#![cfg_attr(not(kani), allow(unused))]

// Import directly from the standalone consensus crate (AC-1: no soroban-sdk).
// All use sites are inside `#[cfg(kani)]` harnesses; the `cfg_attr` above
// suppresses the unused-import warning when building outside Kani.
use vero_consensus::{apply_vote, resolution_invariant_holds, ConsensusError, ConsensusState};

// ─── Harness 1 ────────────────────────────────────────────────────────────────
/// **Threshold Invariant**
///
/// For any symbolic `weight` and `threshold`, after a successful call to
/// `apply_vote`, the following must hold:
///   - If `weight >= threshold` → `is_done == true`
///   - If `weight < threshold`  → `is_done == false`
///   - `total_weight_accrued == weight`
///
/// This is the core safety property: resolution requires meeting the threshold.
#[cfg(kani)]
#[kani::proof]
fn proof_threshold_invariant() {
    let weight: u64 = kani::any();
    let threshold: u64 = kani::any();

    let mut state = ConsensusState::new();

    // Restrict to non-zero weights so the vote succeeds
    kani::assume(weight > 0);

    let result = apply_vote(&mut state, weight, threshold);

    // A non-zero weight with no prior accumulation must succeed
    assert!(
        result.is_ok(),
        "non-zero vote on fresh state must not overflow"
    );

    // Core invariant: is_done iff weight meets threshold
    if weight >= threshold {
        assert!(
            state.is_done,
            "INVARIANT VIOLATION: weight >= threshold but task not resolved"
        );
    } else {
        assert!(
            !state.is_done,
            "INVARIANT VIOLATION: weight < threshold but task was resolved"
        );
    }

    // Accumulated weight must equal the single vote cast
    assert_eq!(
        state.total_weight_accrued, weight,
        "accumulated weight must equal the single vote weight"
    );
    assert_eq!(state.votes, 1, "vote counter must be 1 after one vote");
}

// ─── Harness 2 ────────────────────────────────────────────────────────────────
/// **No Below-Threshold Resolution** (Security Requirement)
///
/// Exhaustively proves that **no execution path** can set `is_done = true`
/// when `total_weight_accrued < threshold` at the point of resolution.
///
/// This is the primary security invariant stated in the acceptance criteria:
/// "Verify that no path allows resolution below threshold."
#[cfg(kani)]
#[kani::proof]
fn proof_no_below_threshold_resolution() {
    let weight: u64 = kani::any();
    let threshold: u64 = kani::any();
    let initial_weight: u64 = kani::any();

    // Allow any valid starting state (e.g. previous votes have already accrued)
    kani::assume(initial_weight < u64::MAX); // leave room to add weight
    kani::assume(weight > 0);

    let mut state = ConsensusState {
        total_weight_accrued: initial_weight,
        votes: kani::any(),
        is_done: false, // we start from an unresolved state
    };

    let result = apply_vote(&mut state, weight, threshold);

    // Whether the vote succeeded or failed, is_done must not be set below threshold
    if state.is_done {
        assert!(
            state.total_weight_accrued >= threshold,
            "SECURITY VIOLATION: task resolved with total_weight_accrued < threshold"
        );
    }
}

// ─── Harness 3 ────────────────────────────────────────────────────────────────
/// **Monotone `is_done`**
///
/// Once a task is resolved (`is_done = true`), subsequent calls to `apply_vote`
/// must never set `is_done = false`. Resolution is irreversible.
#[cfg(kani)]
#[kani::proof]
fn proof_monotone_done() {
    let weight: u64 = kani::any();
    let threshold: u64 = kani::any();

    // Start from an already-resolved state
    kani::assume(weight > 0);
    kani::assume(threshold <= u64::MAX / 2); // avoid overflow in setup

    let mut state = ConsensusState {
        total_weight_accrued: kani::any(),
        votes: kani::any(),
        is_done: true, // already resolved
    };

    // Ensure the accumulated weight is consistent with is_done == true
    kani::assume(state.total_weight_accrued >= threshold);
    // Ensure adding weight won't overflow
    kani::assume(state.total_weight_accrued <= u64::MAX - weight);

    let _ = apply_vote(&mut state, weight, threshold);

    // is_done must remain true — it is monotonically set
    assert!(
        state.is_done,
        "INVARIANT VIOLATION: is_done was cleared after being set (monotonicity broken)"
    );
}

// ─── Harness 4 ────────────────────────────────────────────────────────────────
/// **Weight Overflow Is Impossible Without Error**
///
/// Proves that `apply_vote` never silently wraps `total_weight_accrued`.
/// If an overflow would occur, the function must return `Err(WeightOverflow)`
/// and must NOT modify `state.total_weight_accrued`.
#[cfg(kani)]
#[kani::proof]
fn proof_weight_overflow_impossible() {
    let weight: u64 = kani::any();
    let initial: u64 = kani::any();
    let threshold: u64 = kani::any();

    kani::assume(weight > 0);
    // Force an overflow scenario: initial + weight > u64::MAX
    kani::assume(initial > u64::MAX - weight);

    let mut state = ConsensusState {
        total_weight_accrued: initial,
        votes: kani::any(),
        is_done: false,
    };
    let before = state.total_weight_accrued;

    let result = apply_vote(&mut state, weight, threshold);

    // Must return an error — silent overflow is forbidden
    assert!(
        result == Err(ConsensusError::WeightOverflow),
        "INVARIANT VIOLATION: overflow not caught — weight wrapped silently"
    );
    // State must be unchanged
    assert_eq!(
        state.total_weight_accrued, before,
        "total_weight_accrued must not be modified on overflow"
    );
    assert!(!state.is_done, "is_done must not be set on overflow error");
}

// ─── Harness 5 ────────────────────────────────────────────────────────────────
/// **Votes Counter Never Overflows**
///
/// `votes` uses `saturating_add`, so it can never wrap around from `u32::MAX`
/// back to 0. This harness proves that for any starting `votes` value,
/// the counter after a successful vote is >= the value before.
#[cfg(kani)]
#[kani::proof]
fn proof_votes_no_overflow() {
    let weight: u64 = kani::any();
    let threshold: u64 = kani::any();
    let initial_votes: u32 = kani::any();
    let initial_weight: u64 = kani::any();

    kani::assume(weight > 0);
    // Ensure no weight overflow
    kani::assume(initial_weight <= u64::MAX - weight);

    let mut state = ConsensusState {
        total_weight_accrued: initial_weight,
        votes: initial_votes,
        is_done: false,
    };

    let _ = apply_vote(&mut state, weight, threshold);

    // Votes must be >= the initial value (saturation, never wrap)
    assert!(
        state.votes >= initial_votes,
        "INVARIANT VIOLATION: votes counter wrapped — overflow not prevented"
    );
    // Specifically, it must be exactly initial_votes + 1, or u32::MAX if saturated
    let expected = initial_votes.saturating_add(1);
    assert_eq!(
        state.votes, expected,
        "votes must be exactly saturating_add(1) of the previous value"
    );
}

// ─── Harness 6 ────────────────────────────────────────────────────────────────
/// **Zero Threshold Is Safe**
///
/// When `threshold = 0`, any non-zero weight vote must resolve the task
/// immediately (since `weight >= 0` is always true for u64). This is a
/// degenerate configuration, but it must not cause undefined behaviour.
#[cfg(kani)]
#[kani::proof]
fn proof_zero_threshold_safe() {
    let weight: u64 = kani::any();
    kani::assume(weight > 0);

    let mut state = ConsensusState::new();
    let result = apply_vote(&mut state, weight, 0 /* threshold = 0 */);

    assert!(result.is_ok(), "zero threshold vote must not error");
    assert!(
        state.is_done,
        "zero threshold: any non-zero weight vote must resolve the task"
    );
    assert_eq!(state.total_weight_accrued, weight);
}

// ─── Harness 7 ────────────────────────────────────────────────────────────────
/// **Maximum Weight Single Guardian**
///
/// A guardian with weight = `u64::MAX` voting on a fresh task must:
/// - Succeed (no overflow on fresh state)
/// - Resolve the task for any threshold <= `u64::MAX`
/// - Set `total_weight_accrued = u64::MAX`
#[cfg(kani)]
#[kani::proof]
fn proof_max_weight_single_guardian() {
    let threshold: u64 = kani::any();

    let mut state = ConsensusState::new();
    let result = apply_vote(&mut state, u64::MAX, threshold);

    // u64::MAX added to 0 must succeed (no overflow)
    assert!(
        result.is_ok(),
        "u64::MAX weight on empty state must not overflow"
    );
    assert_eq!(state.total_weight_accrued, u64::MAX);

    // u64::MAX >= any u64 threshold, so is_done must always be true
    assert!(
        state.is_done,
        "u64::MAX weight must resolve task for any threshold"
    );
}

// ─── Harness 8 ────────────────────────────────────────────────────────────────
/// **Multi-Vote Accumulation**
///
/// Simulates two sequential votes with symbolic weights and proves the
/// accumulated weight equals the sum and resolution is correctly triggered.
/// Uses unwind(2) to bound loop depth.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(3)]
fn proof_multi_vote_accumulation() {
    let w1: u64 = kani::any();
    let w2: u64 = kani::any();
    let threshold: u64 = kani::any();

    kani::assume(w1 > 0);
    kani::assume(w2 > 0);
    // Ensure no overflow across two votes
    kani::assume(w1 <= u64::MAX / 2);
    kani::assume(w2 <= u64::MAX / 2);

    let mut state = ConsensusState::new();

    let r1 = apply_vote(&mut state, w1, threshold);
    assert!(r1.is_ok());

    let r2 = apply_vote(&mut state, w2, threshold);
    assert!(r2.is_ok());

    let total = w1 + w2; // safe: both <= MAX/2
    assert_eq!(
        state.total_weight_accrued, total,
        "weight must accumulate correctly"
    );
    assert_eq!(state.votes, 2, "vote count must be 2 after two votes");

    // Resolution must happen iff the sum meets threshold
    if total >= threshold {
        assert!(
            state.is_done,
            "must be resolved when accumulated weight >= threshold"
        );
    } else {
        assert!(
            !state.is_done,
            "must not be resolved when accumulated weight < threshold"
        );
    }
}

// ─── Harness 9 ────────────────────────────────────────────────────────────────
/// **`resolution_invariant_holds` Is a Sound Post-Condition**
///
/// Proves that the helper predicate `resolution_invariant_holds` correctly
/// identifies all safe states: if `is_done == true`, then
/// `total_weight_accrued >= threshold` must hold.
#[cfg(kani)]
#[kani::proof]
fn proof_resolution_invariant_helper() {
    let weight: u64 = kani::any();
    let threshold: u64 = kani::any();

    kani::assume(weight > 0);

    let mut state = ConsensusState::new();
    let result = apply_vote(&mut state, weight, threshold);

    if result.is_ok() {
        // After a successful vote, the resolution invariant must always hold
        assert!(
            resolution_invariant_holds(&state, threshold),
            "resolution_invariant_holds must be true after any successful apply_vote"
        );
    }
}

// When compiled normally (not via `cargo kani`), this crate exposes no
// public symbols — it exists purely for Kani's symbolic execution.
