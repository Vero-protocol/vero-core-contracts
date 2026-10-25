# Formal Verification — Vero Core Consensus

This directory contains **formal models** and **safety specifications** for the
Vero Core Contracts' weighted guardian consensus protocol. The specifications
are expressed in K-framework notation (syntax compatible with
[K Framework](https://kframework.org/)) and define the state transitions,
invariants, and safety properties of the core consensus logic.

## Files

| File | Purpose |
|------|---------|
| `consensus-spec.k` | Symbolic weighted-vote state and `applyVote` transition rules |
| `invariants.k` | Helper predicates for threshold, monotonicity, and numeric bounds |
| `proofs.k` | Reachability claims checked by `kprove` against the model |
| `build.sh` | Build + verification script (Linux / macOS) |
| `build.ps1` | Build + verification script (Windows — PowerShell equivalent of `build.sh`) |

## Protocol Model

The consensus state machine is modelled as:

```
State = (total_weight_accrued: u64, votes: u32, is_done: bool)
```

Transitions:
- `applyVote(weight, threshold)` — apply a single guardian vote
  - Pre: weight > 0
  - Rejects: weight = 0 (ZeroWeight) or overflow (WeightOverflow)
  - Post: total_weight_accrued += weight, votes++. If total >= threshold: is_done = true

## Safety Invariants (Specified)

1. **Threshold Invariant**: `is_done = true` ⟹ `total_weight_accrued ≥ threshold`
2. **No Below-Threshold Resolution**: No execution path sets `is_done` when `total < threshold`
3. **Monotonicity**: Once `is_done = true`, it never becomes false
4. **No Silent Overflow**: `checked_add` catches all u64 overflows before mutation
5. **Vote Counter Safety**: `votes` saturates at `u32::MAX`, never wraps
6. **Zero Threshold Safety**: threshold=0 is well-defined (first vote resolves)
7. **Accumulation Correctness**: Multiple votes sum correctly

## Running Verification

### Kani (Rust model checker)

```bash
cargo kani --manifest-path verification/Cargo.toml
```

### K Framework (deductive verification)

Requires [K Framework 6.0+](https://github.com/runtimeverification/k) installed.

#### Linux / macOS

```bash
# From the repository root — runs build.sh
make proofs

# Or directly from the proofs/ directory
bash build.sh
```

#### Windows (PowerShell)

```powershell
# From the repository root — runs build.ps1 via PowerShell
make proofs-windows

# Or directly from the proofs/ directory
powershell -ExecutionPolicy Bypass -File build.ps1
```

#### Manual steps (any platform)

```bash
# Compile the specification
kompile proofs/consensus-spec.k --backend llvm

# Verify all claims
kprove proofs/proofs.k proofs/consensus-spec.k
```

## Keeping Rust and K proofs in sync

The **source of truth** for vote arithmetic is [`consensus/src/lib.rs`](../consensus/src/lib.rs),
which defines `ConsensusState`, `ConsensusError`, and `apply_vote`.
[`src/consensus.rs`](../src/consensus.rs) merely re-exports these types; it does
not implement the transition. The Soroban-facing voting logic (authorization,
reputation weights, persistence, resolution, and vault release) lives in
[`src/contracts/voting.rs`](../src/contracts/voting.rs).

Review this mapping whenever consensus behavior changes:

| Rust change | K files requiring review | Runtime / Kani counterparts |
|---|---|---|
| Zero-weight rejection, accumulator overflow, or return semantics in `consensus/src/lib.rs` | `consensus-spec.k` transition/error rules; `proofs.k` zero-weight and overflow claims | `tests/consensus.rs`, `tests/safety_invariants.rs`, `verification/src/lib.rs` |
| Weight accrual and resolution threshold comparison | `consensus-spec.k` transition; `invariants.k` threshold predicates; `proofs.k` threshold claims | `tests/consensus.rs`, `tests/safety_invariants.rs` |
| `votes` saturation or monotonic `is_done` | `consensus-spec.k` counter/state rules; `invariants.k` bounds and monotonicity; `proofs.k` monotonicity claim | `tests/property_tests.rs`, `tests/safety_invariants.rs`, `verification/src/lib.rs` |
| Changes to voting flow in `src/contracts/voting.rs` | Decide whether inputs or guarantees assumed by the pure K transition have changed; if so, update the corresponding model and claims | `tests/test.rs` and related integration tests |

### Checklist for a consensus-related pull request

1. **Review the Rust behavior.** Look at the pure `apply_vote` implementation,
   not just its `src/consensus.rs` re-export. Identify any change to error
   handling, accumulated weight, thresholds, the vote count, or resolution state.
2. **Review all three K layers.** Keep the transition model
   (`consensus-spec.k`), invariant helpers (`invariants.k`), and reachability
   claims (`proofs.k`) consistent with the changed semantics. If no K file
   needs changing, explain why in the pull request description.
3. **Update complementary checks.** Add or update Rust tests and the Kani
   harnesses in `verification/src/lib.rs` as needed. Those checks are distinct
   from K proofs and do not automatically validate the K specification.
4. **Run relevant Rust tests** from the repository root:

   ```bash
   cargo test --test consensus
   cargo test --test safety_invariants
   cargo test --test property_tests
   ```

5. **Run the K proofs** after installing [K Framework 6.0+](https://github.com/runtimeverification/k)
   and making `kompile` and `kprove` available on `PATH`:

   ```bash
   # Linux / macOS, from the repository root
   make proofs

   # Windows PowerShell, from the repository root
   make proofs-windows
   ```

   These targets invoke `proofs/build.sh` or `proofs/build.ps1`, compile the
   model, and attempt the claims in `proofs.k`. Investigate errors or
   mismatches rather than treating an attempted run as successful verification.
6. **Report the actual results.** State which K files and claims were reviewed,
   which Rust tests ran, and the K prover command and outcome. If K was not
   installed or the run did not complete, say **"K proofs not run"** and
   request verification by a maintainer with the tool installed.

### What CI does (and does not) verify

The current `.github/workflows/ci.yml` runs Rust formatting, linting, builds,
and tests, but **does not execute K Framework's `kompile` or `kprove`**. This
issue adds a documented review/verification process, **not** a K proof CI gate.
Passing `cargo test` is not evidence that K proof claims passed.

[Kani](https://github.com/model-checking/kani) is a separate Rust model checker.
The Kani proof harnesses live in `verification/src/lib.rs` and require their
own execution (after setting up Kani), from the repository root:

```bash
cargo kani --manifest-path verification/Cargo.toml
```

The old `cargo test --test proof_harness` instruction is not a real test target
in this repository. Routine `cargo test` runs runtime invariant tests, not
Kani's formal proof harnesses. Report Kani proofs as verified only after a
successful `cargo kani` run.
