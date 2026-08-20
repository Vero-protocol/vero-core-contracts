// K Framework Specification — Vero Consensus Logic
// ===================================================
// This file defines the state machine, transitions, and sorts for the
// weighted guardian consensus protocol used in Vero Core Contracts.
//
// The model corresponds 1:1 to the pure Rust implementation in
// src/consensus.rs (ConsensusState, apply_vote, ConsensusError).
//
// Syntax: K Framework 6.0+ (LLVM backend)

requires "domains.k"

module CONSENSUS-SPEC
    imports DOMAINS

    // ─── State Sort ───────────────────────────────────────────────────────────
    // Mirrors ConsensusState from src/consensus.rs.

    syntax ConsensusState ::= consensusState(
        totalWeightAccrued: Int,  // u64 equivalent (0 .. 2^64-1)
        votes:              Int,  // u32 equivalent (0 .. 2^32-1)
        isDone:             Bool
    )

    // ─── Error Sort ───────────────────────────────────────────────────────────
    // Mirrors ConsensusError.

    syntax ConsensusError ::= ZeroWeight
                           | WeightOverflow

    // ─── Result Sort ──────────────────────────────────────────────────────────
    // Either OK(ConsensusState) or error.

    syntax ConsensusResult ::= ok(ConsensusState)
                              | error(ConsensusError)

    // ─── Helper Predicates ─────────────────────────────────────────────────────
    // Bounds checks ensuring values fit within u64 / u32 ranges.

    syntax Bool ::= inU64Range(Int)  [function, smtlib(inU64Range)]
    rule inU64Range(X) => X >= 0 andBool X <= 18446744073709551615  // 2^64 - 1

    syntax Bool ::= inU32Range(Int)  [function, smtlib(inU32Range)]
    rule inU32Range(X) => X >= 0 andBool X <= 4294967295           // 2^32 - 1

    // ─── Transition: applyVote ───────────────────────────────────────────────
    // Corresponds to apply_vote(&mut ConsensusState, weight: u64, threshold: u64)
    //
    // Precondition:  weight > 0
    // Postcondition: see reachability claims in proofs.k

    syntax ConsensusResult ::= applyVote(
        state:     ConsensusState,
        weight:    Int,
        threshold: Int
    ) [function, functional]

    rule applyVote(
          consensusState(ACC, VOTES, DONE),
          WEIGHT, THRESHOLD
        ) => error(ZeroWeight)
      requires WEIGHT <= 0

    rule applyVote(
          consensusState(ACC, VOTES, DONE),
          WEIGHT, THRESHOLD
        ) => error(WeightOverflow)
      requires WEIGHT > 0
       andBool notBool inU64Range(ACC +Int WEIGHT)

    rule applyVote(
          consensusState(ACC, VOTES, DONE),
          WEIGHT, THRESHOLD
        ) => ok(consensusState(
                ACC +Int WEIGHT,
                minInt(VOTES +Int 1, 4294967295),  // saturating_add
                (ACC +Int WEIGHT >=Int THRESHOLD)  // threshold check
             ))
      requires WEIGHT > 0
       andBool inU64Range(ACC +Int WEIGHT)
       andBool inU32Range(VOTES +Int 1)

endmodule