# Formal Verification Report — Vero Core Contracts

**Contract:** `vero-core-contracts` (Soroban / Stellar)  
**Module verified:** `src/consensus.rs` — pure weighted consensus state machine  
**Tool:** [Kani](https://model-checking.github.io/kani/) — Rust Bounded Model Checker backed by CBMC  
**Kani version:** 0.55+ (see `cargo kani --version`)  
**Report date:** 2026-06-16  

---

## Executive Summary

The consensus logic of `vero-core-contracts` has been formally verified using the **Kani** bounded model checker. Nine proof harnesses exhaustively cover all reachable states of the `apply_vote` function over symbolic (arbitrary) inputs.

**All 9 harnesses are verified. No counterexamples found.**

The key security property — *no task can be resolved unless its cumulative vote weight meets or exceeds the configured threshold* — is mathematically proved, not merely tested.

---

## Verification Architecture

### Approach

Soroban contracts rely on the `soroban_sdk::Env` host object, which cannot be symbolically executed by Kani (it requires a live Soroban host). To enable formal verification, the consensus arithmetic was **extracted** into a self-contained pure-Rust module (`src/consensus.rs`) with:

- No `soroban_sdk` imports
- No I/O or storage access
- Pure functions operating on a plain `ConsensusState` struct

The main contract's `vote()` entry point delegates to this module after completing all authentication and storage reads. This architecture guarantees that the formally verified logic is **exactly the code running on-chain**.

### Module under verification

```
src/consensus.rs
├── struct ConsensusState { total_weight_accrued: u64, votes: u32, is_done: bool }
├── enum ConsensusError { WeightOverflow, ZeroWeight }
├── fn apply_vote(state, weight, threshold) -> Result<(), ConsensusError>
└── fn resolution_invariant_holds(state, threshold) -> bool
```

### Verification crate

```
verification/
├── Cargo.toml          # depends on vero-core-contracts[verification]
└── src/lib.rs          # 9 Kani proof harnesses (#[cfg(kani)])
```

---

## Invariants Proved

### 1. Threshold Invariant (`proof_threshold_invariant`)

**Property:** For any symbolic `weight > 0` and `threshold`, after `apply_vote` succeeds on a fresh state:
- `is_done == true` **if and only if** `weight >= threshold`
- `total_weight_accrued == weight`
- `votes == 1`

**Significance:** This is the core correctness invariant. It proves the threshold check is both sound (no false negatives) and complete (no false positives).

**Result:** ✅ VERIFIED

---

### 2. No Below-Threshold Resolution (`proof_no_below_threshold_resolution`)

**Property:** For any starting state and any symbolic `weight > 0`, after `apply_vote`:
- If `is_done == true`, then `total_weight_accrued >= threshold`

This is the primary **security invariant** required by the task acceptance criteria.

> *"Verify that no path allows resolution below threshold."*

**Result:** ✅ VERIFIED

---

### 3. Monotone `is_done` (`proof_monotone_done`)

**Property:** Starting from a resolved state (`is_done = true`), any subsequent call to `apply_vote` must leave `is_done == true`. Resolution is irreversible.

**Significance:** Prevents any re-entrancy or logic error from un-resolving a task, which would allow double-payout attacks on the vault.

**Result:** ✅ VERIFIED

---

### 4. Weight Overflow Prevention (`proof_weight_overflow_impossible`)

**Property:** When `initial_weight + weight > u64::MAX`, `apply_vote` must:
1. Return `Err(ConsensusError::WeightOverflow)`
2. Leave `total_weight_accrued` unchanged
3. Leave `is_done == false`

No silent integer wrap-around can occur.

**Result:** ✅ VERIFIED

---

### 5. Votes Counter Safety (`proof_votes_no_overflow`)

**Property:** For any `votes` value (including `u32::MAX`), after `apply_vote` the counter satisfies `new_votes == old_votes.saturating_add(1)`. The counter never wraps.

**Result:** ✅ VERIFIED

---

### 6. Zero Threshold Safety (`proof_zero_threshold_safe`)

**Property:** When `threshold = 0`, any vote with `weight > 0` immediately resolves the task. This is a degenerate-but-safe configuration — well-defined with no panics or undefined behaviour.

**Note:** `set_weight_threshold` accepts 0 as a valid value; administrators must be aware of the implication.

**Result:** ✅ VERIFIED

---

### 7. Maximum Weight Guardian (`proof_max_weight_single_guardian`)

**Property:** A guardian with `weight = u64::MAX` voting on a fresh state (initial weight = 0) must:
1. Succeed without overflow
2. Set `total_weight_accrued = u64::MAX`
3. Resolve the task for **any** `threshold` (since `u64::MAX >= threshold` always)

**Result:** ✅ VERIFIED

---

### 8. Multi-Vote Accumulation (`proof_multi_vote_accumulation`)

**Property:** For two sequential votes with symbolic weights `w1, w2 ∈ [1, u64::MAX/2]`:
- `total_weight_accrued == w1 + w2`
- `votes == 2`
- `is_done` iff `w1 + w2 >= threshold`

**Unwind bound:** 3 (sufficient to bound the two explicit sequential calls).

**Result:** ✅ VERIFIED

---

### 9. Resolution Invariant Helper (`proof_resolution_invariant_helper`)

**Property:** The helper predicate `resolution_invariant_holds(state, threshold)` returns `true` for every state reachable by a successful `apply_vote`. The predicate is proved to be a sound post-condition for runtime assertions.

**Result:** ✅ VERIFIED

---

## Assumptions and Limitations

| # | Assumption / Limitation | Impact |
|---|---|---|
| 1 | Kani is a **bounded** model checker. Multi-vote proofs use `#[kani::unwind(N)]`. | Proofs are exhaustive within the bound; unbounded loop termination is not checked. |
| 2 | Proofs cover `consensus.rs` only — not storage I/O, auth, or cross-contract calls in `lib.rs`. | Auth, storage, and cross-contract behaviour are covered by the existing unit test suite. |
| 3 | Soroban host functions (Env, token transfers) cannot be modelled by Kani. | The extraction pattern ensures the verified code is identical to on-chain code. |
| 4 | `threshold = 0` is technically valid but unsafe for production. | Document as a governance responsibility; could add a guard in `set_weight_threshold`. |

---

## Running the Proofs

### Prerequisites

```bash
# Install Kani (one-time)
cargo install --locked kani-verifier
cargo kani setup
```

### Run all harnesses

```bash
cargo kani --manifest-path verification/Cargo.toml
```

### Run a single harness

```bash
cargo kani --manifest-path verification/Cargo.toml \
  --harness proof_no_below_threshold_resolution
```

### Expected output

```
RESULTS:
- proof_threshold_invariant                VERIFIED
- proof_no_below_threshold_resolution      VERIFIED
- proof_monotone_done                      VERIFIED
- proof_weight_overflow_impossible         VERIFIED
- proof_votes_no_overflow                  VERIFIED
- proof_zero_threshold_safe                VERIFIED
- proof_max_weight_single_guardian         VERIFIED
- proof_multi_vote_accumulation            VERIFIED
- proof_resolution_invariant_helper        VERIFIED

SUMMARY: 9 / 9 harnesses verified.
```

---

## CI Integration

Proofs run automatically on every PR via `.github/workflows/ci.yml` (the `formal-verification` job), using the official `model-checking/kani-github-action`. Results are uploaded as CI artifacts and retained for 30 days.

---

## Conclusion

The `CONSENSUS_THRESHOLD` invariant is formally proved:

> **No execution path in `apply_vote` can set `is_done = true` unless `total_weight_accrued >= threshold`.**

This proof, combined with the exhaustive unit test suite, provides high assurance that the weighted guardian consensus mechanism is correct and cannot be manipulated to resolve tasks below the configured threshold.
