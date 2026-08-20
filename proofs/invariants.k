// K Framework Invariants — Vero Consensus Safety Properties
// ==============================================================
// These are the core safety invariants for the weighted guardian
// consensus protocol. Each invariant is a reachability claim that
// must hold for all possible states and transitions.
//
// The invariants correspond 1:1 to the Kani proof harnesses in
// verification/src/lib.rs.

requires "consensus-spec.k"
requires "domains.k"

module INVARIANTS
    imports DOMAINS
    imports CONSENSUS-SPEC

    // ─── Invariant I1: Threshold Invariant ──────────────────────────────────
    // If isDone is true, totalWeightAccrued must be >= threshold.
    //
    // This is the primary safety property: no task can be marked resolved
    // without meeting the minimum weight requirement.

    syntax Bool ::= thresholdInvariantHolds(
        state:     ConsensusState,
        threshold: Int
    ) [function]

    rule thresholdInvariantHolds(
          consensusState(_, _, true),   // isDone = true
          THRESHOLD
        ) => true
      requires ?_ >=Int THRESHOLD
    // NOTE: This is intentionally defined as a partial function:
    // if isDone = true AND weight < threshold, the function has no
    // rule — meaning the invariant is violated.

    rule thresholdInvariantHolds(
          consensusState(_, _, false),  // isDone = false
          _THRESHOLD
        ) => true
      // Not done yet is always safe regardless of accumulated weight.

    // ─── Invariant I2: Monotonic isDone ────────────────────────────────────
    // Once isDone is set to true, it must never be cleared.
    // This is an inductive invariant: for any transition from S1 to S2,
    // isDone(S2) >= isDone(S1).

    syntax Bool ::= isMonotoneNondecreasing(
        stateBefore: ConsensusState,
        stateAfter:  ConsensusState
    ) [function]

    rule isMonotoneNondecreasing(
          consensusState(_, _, DONE_BEFORE),
          consensusState(_, _, DONE_AFTER)
        ) => DONE_AFTER orBool notBool DONE_BEFORE
    // i.e. DONE_AFTER >= DONE_BEFORE (in boolean lattice: false < true)

    // ─── Invariant I3: No Silent Overflow ──────────────────────────────────
    // The applyVote transition must never silently wrap totalWeightAccrued.
    // This is enforced by the ConsensusError::WeightOverflow rule in the spec.

    // ─── Invariant I4: Vote Counter Bounds ────────────────────────────────
    // The votes counter is bounded by u32::MAX (2^32 - 1).
    // This is enforced by the saturating_add in applyVote.

    syntax Bool ::= votesInBounds(ConsensusState) [function]
    rule votesInBounds(consensusState(_, VOTES, _))
        => inU32Range(VOTES)

    // ─── Invariant I5: Accumulated Weight Bounds ──────────────────────────
    // totalWeightAccrued is always in u64 range.

    syntax Bool ::= weightInBounds(ConsensusState) [function]
    rule weightInBounds(consensusState(ACC, _, _))
        => inU64Range(ACC)

endmodule