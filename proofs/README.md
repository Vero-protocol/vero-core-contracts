# Formal Verification — Vero Core Consensus

This directory contains **formal models** and **safety specifications** for the
Vero Core Contracts' weighted guardian consensus protocol. The specifications
are expressed in K-framework notation (syntax compatible with
[K Framework](https://kframework.org/)) and define the state transitions,
invariants, and safety properties of the core consensus logic.

## Files

| File | Purpose |
|------|---------|
| `consensus-spec.k` | Main K specification: state, transitions, invariants |
| `invariants.k` | Safety invariants for the consensus protocol |
| `proofs.k` | Proof claims (reachability logic) for each invariant |
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

## Safety Invariants (Proved)

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
cargo kani --manifest-path ../verification/Cargo.toml
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

## Integration

The Kani proofs are integrated into `cargo test` via:

```bash
cargo test --test proof_harness
```

Which runs all Kani proof harnesses defined in `verification/src/lib.rs`.