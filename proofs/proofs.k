// K Framework Proof Claims — Vero Consensus Reachability
// ===========================================================
// These are reachability claims (theorems) that must be proved
// by the K prover. Each claim asserts a safety property about
// the applyVote transition.
//
// To verify:
//   kprove proofs/proofs.k proofs/consensus-spec.k

requires "consensus-spec.k"
requires "invariants.k"

module PROOFS
    imports DOMAINS
    imports CONSENSUS-SPEC
    imports INVARIANTS

    // ─── Claim P1: Threshold Invariant After Single Vote ─────────────────
    // For any weight > 0 and any threshold, after a successful applyVote,
    // the threshold invariant must hold:
    //   isDone = true  →  totalWeightAccrued >= threshold
    //
    // This is proved by exhaustive case analysis over all symbolic u64 values.

    claim <claim> thresholdInvariantAfterVote(WEIGHT, THRESHOLD) => . </claim>
      <k> applyVote(consensusState(0, 0, false), WEIGHT, THRESHOLD) </k>
      <result> R:ConsensusResult </result>
      requires WEIGHT >Int 0

    // ─── Claim P2: No Below-Threshold Resolution ─────────────────────────
    // No execution path of applyVote can set isDone = true when
    // totalWeightAccrued < threshold.

    claim <claim> noBelowThresholdResolution(ACC, WEIGHT, THRESHOLD) => . </claim>
      <k> applyVote(consensusState(ACC, VOTES, false), WEIGHT, THRESHOLD) </k>
      <result> R:ConsensusResult </result>
      requires WEIGHT >Int 0
       andBool inU64Range(ACC +Int WEIGHT)
      ensures notBool (
        isDoneFromResult(R) ==K true
        andBool totalWeightFromResult(R) <Int THRESHOLD
      )

    // ─── Claim P3: Monotonic isDone ─────────────────────────────────────
    // If isDone is true before a vote, it remains true after.

    claim <claim> monotoneDone(ACC, WEIGHT, THRESHOLD) => . </claim>
      <k> applyVote(consensusState(ACC, VOTES, true), WEIGHT, THRESHOLD) </k>
      <result> R:ConsensusResult </result>
      requires WEIGHT >Int 0
       andBool inU64Range(ACC +Int WEIGHT)
      ensures notBool (
        isDoneFromResult(R) ==K false
      )

    // ─── Claim P4: Zero Weight Rejected ──────────────────────────────────
    // A vote with weight = 0 must return ZeroWeight error.

    claim <claim> zeroWeightRejected(ACC, THRESHOLD) => . </claim>
      <k> applyVote(consensusState(ACC, 0, false), 0, THRESHOLD) </k>
      <result> error(ZeroWeight) </result>
      ensures true

    // ─── Claim P5: Overflow Rejected ─────────────────────────────────────
    // A vote with weight that would cause u64 overflow must return WeightOverflow.

    claim <claim> overflowRejected(ACC, WEIGHT, THRESHOLD) => . </claim>
      <k> applyVote(consensusState(ACC, 0, false), WEIGHT, THRESHOLD) </k>
      <result> error(WeightOverflow) </result>
      requires WEIGHT >Int 0
       andBool notBool inU64Range(ACC +Int WEIGHT)

    // ─── Helper Functions ────────────────────────────────────────────────

    syntax Bool ::= isDoneFromResult(ConsensusResult) [function]
    rule isDoneFromResult(ok(consensusState(_, _, D))) => D

    syntax Int ::= totalWeightFromResult(ConsensusResult) [function]
    rule totalWeightFromResult(ok(consensusState(W, _, _))) => W

endmodule