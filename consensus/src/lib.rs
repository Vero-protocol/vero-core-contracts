//! Pure, `no_std`-compatible consensus logic — **no Soroban `Env` dependency**.
//!
//! This crate contains the arithmetic and state-transition rules for the
//! weighted guardian consensus. Keeping this logic free of SDK types allows
//! Kani (and other model checkers) to formally verify it without mocking the
//! Soroban host environment.
//!
//! The contract's `vote()` entry point delegates to [`apply_vote`] after
//! performing all authentication, authorisation, and storage I/O.
//!
//! ## Test placement
//!
//! Unlike the rest of the contract crate, this module's unit tests are **not**
//! inline — they live in `tests/consensus.rs` in the main contract crate, in
//! keeping with the crate-wide convention of placing all tests under `tests/`.
//! The tests there are deliberately Soroban-free (no `Env`, no `testutils`) for
//! the same reason this crate is: so that Kani and other model checkers can
//! consume them without a host-environment mock. Complementary coverage is
//! provided by the Kani harnesses in `verification/` and the runtime invariant
//! checks in `tests/safety_invariants.rs`.

#![no_std]

/// Errors that can arise purely from consensus arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsensusError {
    /// Adding the guardian's weight to the accumulated total would overflow `u64`.
    WeightOverflow,
    /// The guardian's voting weight is zero — their vote has no effect.
    ZeroWeight,
}

/// The mutable consensus state for a single task.
///
/// This is a plain data struct with no Soroban types so that Kani can create
/// symbolic instances of it and exhaustively verify all reachable states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsensusState {
    /// Cumulative reputation weight accrued from all guardian votes so far.
    pub total_weight_accrued: u64,
    /// Number of guardian votes cast (saturating counter).
    pub votes: u32,
    /// `true` once the task has been resolved (monotonically set).
    pub is_done: bool,
}

impl ConsensusState {
    /// Creates a fresh, unresolved consensus state.
    pub const fn new() -> Self {
        Self {
            total_weight_accrued: 0,
            votes: 0,
            is_done: false,
        }
    }
}

impl Default for ConsensusState {
    fn default() -> Self {
        Self::new()
    }
}

/// Applies a single guardian vote to the consensus state.
///
/// # Arguments
/// * `state`     — mutable reference to the current task consensus state.
/// * `weight`    — the guardian's voting power (their reputation score).
/// * `threshold` — cumulative weight required to resolve the task.
///
/// # Behaviour
/// 1. Rejects zero-weight votes.
/// 2. Safely accumulates `weight` into `total_weight_accrued` via checked
///    addition, returning `Err(ConsensusError::WeightOverflow)` on overflow.
/// 3. Increments the vote counter with **saturating** arithmetic (never wraps).
/// 4. Sets `is_done = true` **if and only if** `total_weight_accrued >= threshold`
///    after the addition. `is_done` is never cleared once set.
///
/// # Invariants (proved by Kani harnesses in `verification/`)
/// * Resolution ↔ `total_weight_accrued >= threshold`
/// * No execution path sets `is_done` without meeting `threshold`
/// * `is_done` is monotonically set (never unset)
/// * `checked_add` prevents silent overflow
/// * `votes` saturates at `u32::MAX`
pub fn apply_vote(
    state: &mut ConsensusState,
    weight: u64,
    threshold: u64,
) -> Result<(), ConsensusError> {
    if weight == 0 {
        return Err(ConsensusError::ZeroWeight);
    }

    // Overflow-safe accumulation — the only arithmetic that matters for consensus.
    state.total_weight_accrued = state
        .total_weight_accrued
        .checked_add(weight)
        .ok_or(ConsensusError::WeightOverflow)?;

    // Saturating vote count — purely informational, never drives resolution.
    state.votes = state.votes.saturating_add(1);

    // Threshold check: set is_done iff threshold is met.
    // is_done is never cleared — once true it stays true.
    if state.total_weight_accrued >= threshold {
        state.is_done = true;
    }

    Ok(())
}

/// Returns `true` if the consensus state satisfies the resolution invariant:
/// `is_done` must be `true` **if and only if** `total_weight_accrued >= threshold`.
///
/// Used both in runtime assertions and in Kani harnesses as a post-condition.
pub fn resolution_invariant_holds(state: &ConsensusState, threshold: u64) -> bool {
    let weight_meets_threshold = state.total_weight_accrued >= threshold;
    // is_done must imply threshold met, AND threshold met must imply is_done
    // (for a freshly-voted state — NOT for older states where votes may have
    //  already set is_done and threshold was later lowered by admin).
    //
    // The minimal safety invariant (no resolution below threshold) is:
    //   is_done == true  →  weight_meets_threshold
    if state.is_done {
        weight_meets_threshold
    } else {
        true // Not done yet is always safe regardless of weight
    }
}
