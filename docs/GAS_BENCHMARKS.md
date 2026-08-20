# Gas Benchmarks and Ledger Resource Reference

This document defines the baseline gas (CPU instruction) and ledger storage costs for Vero Protocol core contract entry points. It serves as a benchmark reference to detect gas regressions during PR reviews and contract refactorings, as outlined in `plan.md`.

---

## 1. Metering & Cost Model Overview

Calibrated against Stellar Protocol 21 / Soroban metering schedule:

- **Base Invocation Overhead**: ~1,000,000 CPU instructions
- **Instance Storage Read**: ~50,000 CPU instructions / entry
- **Instance Storage Write**: ~150,000 CPU instructions / entry
- **Cross-Contract Call Overhead**: ~500,000 CPU instructions (per external contract call)
- **WASM Upgrade / Deployer**: ~2,000,000 CPU instructions (platform fixed)
- **Event Emission**: ~30,000 CPU instructions per topic/value pair

All instruction costs map 1-to-1 to Soroban's instruction counter and ledger fee schedule. Constants are conservatively calibrated in `src/gas.rs` to provide safe upper bounds for gas limits. The per-op arithmetic in `src/gas.rs` is only a rough breakdown; for operations covered by `tests/gas_budget.rs`, the constant is calibrated against the measured CPU instruction cost instead.

---

## 2. Per-Entry-Point Cost Benchmarks

| Entry Point | Cost Constant (`src/gas.rs`) | CPU Instructions Limit | Ledger Reads / Writes Breakdown |
|---|---|---|---|
| `register_task` | `COST_REGISTER_TASK` | **1,300,000** | Base + reentrancy lock write + `has()` check + role check + task write + index write + unlock write + event |
| `cancel_task` | `COST_CANCEL_TASK` | **1,650,000** | Paused read + role read + task read + task write + event |
| `vote` | `COST_VOTE` | **3,200,000** | Base + circuit-breaker read + 5 storage reads (token, threshold, balance, voted, task) + reentrancy lock/unlock + voted write + task write + event + fault-isolated vault call |
| `add_guardian` | `COST_ADD_GUARDIAN` | **1,400,000** | Paused/role reads + guardian + `AllGuardians` + dense-index writes + event |
| `remove_guardian` | `COST_REMOVE_GUARDIAN` | **2,150,000** | Paused/role/`has()` reads + guardian remove + `AllGuardians` read/write + dense-index swap-remove + event |
| `set_reputation` | `COST_SET_REPUTATION` | **1,600,000** | Paused read + role read + guardian check + reputation write + event |
| `set_weight_threshold` | `COST_SET_WEIGHT_THRESHOLD` | **1,250,000** | Paused read + role read + threshold write + event |
| `set_vault_address` | `COST_SET_VAULT_ADDRESS` | **1,250,000** | Paused read + role read + vault write + event |
| `lock_tokens` | `COST_LOCK_TOKENS` | **5,000,000** | Base + paused read + auth + token read + fee_bps read + treasury read + 2x token transfers + balance read/write + event |
| `request_unlock` | `COST_REQUEST_UNLOCK` | **1,250,000** | Paused read + timelock write + event |
| `unlock_tokens` | `COST_UNLOCK_TOKENS` | **5,000,000** | Base + has() + guardian read + balance read + fee read + treasury read + 2x token transfers + balance write |
| `resign_guardian` | `COST_RESIGN_GUARDIAN` | **5,000,000** | Base + has() + guardian status write + balance/fee/treasury reads + 2x token transfers + balance write |
| `start_reward_stream` | `COST_START_REWARD_STREAM` | **1,330,000** | Base + circuit-breaker read + task read + stream has() + cross-contract Drips call + stream write + event |
| `toggle_pause` | `COST_TOGGLE_PAUSE` | **1,450,000** | Role read + paused read + paused write + event |
| `pause` | `COST_PAUSE` | **1,450,000** | Role read + paused write + event |
| `unpause` | `COST_UNPAUSE` | **1,600,000** | Role read + paused write + event |

---

## 3. Estimating Costs On-Chain & In SDK

The contract exposes `get_estimated_cost(operation: Operation) -> u64` — implemented in `src/gas.rs` and exposed as a read-only entrypoint on `VeroContract`.
Callers can query cost estimates for any supported operation before transaction submission to parameterize transaction gas limits accurately.

---

## 4. How to Run & Regenerate Benchmarks Locally

Budget assertions and gas consumption are verified via Soroban environment budget tests in `tests/gas_budget.rs`.

### Running the Gas Budget Test Suite:
```bash
cargo test --test gas_budget
```

### Running with Output / Metrics Display:
```bash
cargo test --test gas_budget -- --nocapture
```

If any entry point exceeds its documented constant in `src/gas.rs`, the test assertion fails with:
```
<op_name> cost (<actual_cost>) exceeds documented limit (<max_cost>)
```
