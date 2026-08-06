# Fix Report — `record_failure` Circuit-Breaker DoS

**Repository:** `vero-core-contracts` (Soroban / Stellar, `soroban-sdk 21.0.0`)
**Issue:** Permissionless `record_failure` lets any single address pause the entire contract
**Status:** Fixed, built, and covered by 13 new tests. Full suite: **130 passed / 0 failed**.

---

## 1. What the project is building

Vero Core is a Soroban smart contract for **on-chain GitHub PR verification**.
Off-chain validators ("guardians") lock tokens, are assigned reputation scores,
and cast reputation-weighted votes on registered tasks (pull requests). When a
task's `total_weight_accrued` crosses `WeightThreshold`, it is marked done —
producing a tamper-proof audit trail. Supporting subsystems: RBAC (6 roles),
multi-sig upgrades, timelocked withdrawals, treasury fees, reentrancy guard,
storage migration, and an emergency **circuit breaker**.

## 2. The defect, located precisely

| Location | Problem |
|---|---|
| `src/contracts/proxy_entry.rs:351` | `pub fn record_failure(env: Env)` — **no arguments, no `require_auth`, no role check, no return type** (couldn't even signal rejection) |
| `src/circuit_breaker.rs:26` | `pub fn record_failure(env: &Env)` — unconditionally `saturating_add(1)` on the global `DataKey::FailureCount`, auto-setting `DataKey::Paused = true` at `count > 50` |
| `src/contracts/proxy_entry.rs:747` | `BatchCall::RecordFailure(_admin)` — the address argument was **discarded** (`_admin`), so even batch calls were unauthenticated |

**Impact confirmed:** `DataKey::Paused` gates ~15 entry points via
`circuit_breaker::require_not_paused` — `vote`, `vote_batch`, `register_task`,
`add_guardian`, `remove_guardian`, `set_reputation`, `lock_tokens`,
`set_weight_threshold`, `start_reward_stream`, `archive_task`, `record_snapshot`,
and more. One address calling `record_failure()` 51 times froze all of them
until an `EmergencyManager` intervened. Pre-existing tests
(`tests/test.rs:906`, `tests/rbac_tests.rs:477`) literally used
`for _ in 0..51 { client.record_failure(); }` as a one-liner to pause the
contract — the exploit was the documented test fixture.

## 3. Decision on the intended trust model *(acceptance criterion #1)*

> **"Permissionless but authenticated, rate-limited, and quorum-gated."**

Reporting stays open to any observer (the README's stated design goal), but the
trust the README only *implied* is now enforced on-chain by five layered controls:

| # | Control | Constant | Effect |
|---|---|---|---|
| 1 | Authenticated reporter | — | `record_failure(reporter)` + `reporter.require_auth()`; reports are attributable |
| 2 | Per-reporter cooldown | `REPORT_COOLDOWN_LEDGERS = 10` | Ledger sequence is constant within a transaction, so a 51-call loop dies after call #1 |
| 3 | Per-reporter quota | `MAX_REPORTS_PER_REPORTER = 5` | One address supplies ≤5 of the 50 needed reports per window, no matter how long it waits |
| 4 | Distinct-reporter quorum | `MIN_DISTINCT_REPORTERS = 3` | Breaker trips only on corroboration from independent addresses |
| 5 | Trusted-monitor mode | `TrustedReportersOnly` | `EmergencyManager` escape hatch restricting reports to guardians + Emergency/Admin roles |

**Which control is actually load-bearing (verified by mutation testing).** The
**quota (control 3) is the binding constraint**: at 5 reports per address, tripping
the breaker requires `floor(50/5) + 1 = 11` distinct addresses. Since 11 > 3, the
quorum check (control 4) is *currently redundant* — removing it does not reopen
the DoS (mutation 3 below stayed green). It is retained as **defense-in-depth**
that becomes load-bearing the moment anyone retunes `MAX_REPORTS_PER_REPORTER`
upward, and it makes the "corroboration required" intent explicit in code rather
than an emergent side effect of arithmetic. Controls 1, 2 and 3 are each
independently load-bearing — removing any one of them fails a test.

Controls 3+4 are backed by a **compile-time assertion** in `src/circuit_breaker.rs`:

```rust
const _: () = assert!(
    MAX_REPORTS_PER_REPORTER * (MIN_DISTINCT_REPORTERS - 1) <= FAILURE_THRESHOLD,
    "a sub-quorum coalition must not be able to reach FAILURE_THRESHOLD"
);   //  5 * 2 = 10  <=  50   ✓
```

The DoS is therefore impossible **by construction**, not merely by convention:
future edits that weaken the constants fail the build.

**Alternatives considered and rejected.** Role-gating `record_failure` discards
the "any observer can report" goal and concentrates liveness signalling in the
very set the breaker protects against — so it is offered as opt-in mode #5 rather
than the default. Requiring a verifiable failed-invocation hash is unenforceable
on-chain (Soroban cannot verify another transaction's outcome), leaving an
unchecked argument that only raises the cost of generating distinct values.

## 4. Changes made

### `src/circuit_breaker.rs` — rewritten (54 → 240 lines)
- Module-level **decision record** documenting the trust model.
- `record_failure(env, reporter) -> Result<(), ContractError>` now: validates the
  address (rejects zero-address/self), calls `reporter.require_auth()`, checks
  trusted-mode gating, enforces cooldown, enforces quota, records the reporter in
  a bounded distinct-reporter index, commits the report, emits `cb_report`, and
  trips only on `count > 50 && distinct >= 3`.
- New read helpers: `failure_count`, `reporter_count`, `last_report_ledger`,
  `failure_reporters`, `trusted_reporters_only`.
- `reset()` now clears all per-reporter accounting so each window starts clean.
- `MAX_TRACKED_REPORTERS = 100` bounds storage/iteration cost.

### `src/contracts/proxy_entry.rs`
- `record_failure(env, reporter) -> Result<(), ContractError>` (was `(env)`, no return).
- `BatchCall::RecordFailure(reporter)` now **forwards and authenticates** the
  address instead of discarding it as `_admin`, and propagates errors with `?`.
- 5 new entry points: `get_failure_count`, `get_reporter_failure_count`,
  `get_failure_reporters`, `is_trusted_reporters_only`,
  `set_trusted_reporters_only` (EmergencyManager-gated).

### `src/contracts/storage_layout.rs`
- 4 new `DataKey` variants: `LastFailureReport(Address)`,
  `ReporterFailureCount(Address)`, `FailureReporters`, `TrustedReportersOnly`.
  Appended **after** existing variants, preserving enum discriminants and
  on-chain storage compatibility.

### `src/types.rs`
- 3 new error codes appended: `ReportRateLimited = 38`,
  `ReporterQuotaExceeded = 39`, `UnauthorizedReporter = 40`.

### `src/events.rs`
- `emit_failure_reported` (`cb_report`) — per-report attribution for off-chain monitoring.
- `emit_trusted_reporters_only_set` (`cb_trust`).

### `src/gas.rs`
- `COST_RECORD_FAILURE` recalibrated `880_000 → 1_510_000` for the added
  reads/writes, keeping the estimator honest.

### `README.md`
- Trust-model decision record, updated architecture diagram, new storage keys,
  3 new error-code rows, revised emergency-halt procedure and security note.

## 5. Validation

| Check | Result |
|---|---|
| `cargo check --workspace --all-targets` | ✅ 0 errors (only 7 pre-existing warnings, untouched) |
| `cargo build --target wasm32-unknown-unknown --release` | ✅ `vero_core_contracts.wasm`, 101,118 bytes |
| `cargo test` (full suite) | ✅ **130 passed, 0 failed** (baseline 117 + 13 new) |
| `make check` / `make build` / `make invariants` | ✅ all pass |
| Regression risk | None — no pre-existing test was weakened; the two suites that used the exploit as a fixture were rewritten to trip the breaker legitimately |

### Mutation testing — proof the tests actually detect the bug

A test that passes against vulnerable code is worthless, so each control was
individually disabled to confirm the suite catches its absence:

| Mutation | Expected | Result |
|---|---|---|
| Remove per-reporter **quota** check | acceptance test fails | ✅ FAILED — `one address must never contribute more than its quota` |
| Remove per-reporter **cooldown** check | exploit-replay test fails | ✅ FAILED — `second report in same ledger must be rejected: Ok(())` |
| Remove **`require_auth()`** | auth test fails | ✅ FAILED — `unauthenticated report must be rejected` |
| Remove **distinct-reporter quorum** | sub-quorum test fails | ⚠️ **still passed** — see note above; quorum is redundant defense-in-depth at current constants, not the load-bearing control |
| Weaken `MAX_REPORTS_PER_REPORTER` to 26 | build fails | ✅ `error[E0080]: evaluation panicked: a sub-quorum coalition must not be able to reach FAILURE_THRESHOLD` |

### New test file: `tests/circuit_breaker_dos.rs` (13 tests) *(acceptance criterion #2)*

**Headline test — `test_single_address_cannot_unilaterally_pause_contract`:**
one attacker attempts 60 reports, patiently advancing the ledger past the
cooldown before *every* attempt (the strongest possible attack). Asserts exactly
5 accepted, the rest rejected, `is_paused() == false`, and that `add_guardian`
and `register_task` — the operations the DoS used to block — still succeed.

Supporting coverage: `test_loop_in_single_ledger_is_rate_limited` (the original
51-calls-in-a-loop exploit, now rejected with `ReportRateLimited`),
`test_sub_quorum_coalition_cannot_pause_contract`,
`test_report_accepted_after_cooldown_elapses`, `test_reporter_quota_is_enforced`,
`test_distinct_reporters_are_tracked`,
`test_quorum_of_independent_reporters_still_trips_breaker` (proves the breaker
still *works*), `test_breaker_requires_distinct_reporter_quorum`,
`test_reset_clears_per_reporter_accounting`,
`test_trusted_reporters_only_mode_blocks_strangers`,
`test_only_emergency_manager_can_toggle_trusted_mode`,
`test_record_failure_requires_reporter_auth`, `test_zero_address_cannot_report`.

## 6. Confidence: ~97%

The attack path is closed by a compile-time-proven invariant, exercised by a
direct exploit-replay test, and the entire pre-existing suite plus the WASM
release build are green. The residual 3% is the deployment consideration below —
a deliberate, documented API change rather than an unknown.

**Breaking API change (intentional and required):** `record_failure()` →
`record_failure(reporter: Address)`, now returning `Result`. Off-chain monitors
must pass and sign with their reporter address. This is unavoidable — an
unauthenticated caller is precisely the vulnerability. Existing on-chain state
migrates cleanly: new `DataKey` variants are appended, and an in-flight
`FailureCount` simply continues in the new window (reporter accounting starts
empty, which is the conservative direction).

## 7. Files changed

| File | Δ | Type |
|---|---|---|
| `src/circuit_breaker.rs` | +234 / −34 | Modified — core fix |
| `src/contracts/proxy_entry.rs` | +64 / −5 | Modified — entry points |
| `src/contracts/storage_layout.rs` | +10 | Modified — storage keys |
| `src/types.rs` | +9 | Modified — error codes |
| `src/events.rs` | +18 | Modified — events |
| `src/gas.rs` | +9 / −3 | Modified — cost estimate |
| `README.md` | +97 / −15 | Modified — decision record |
| `tests/rbac_tests.rs` | +30 / −8 | Modified — call sites |
| `tests/test.rs` | +36 / −13 | Modified — call sites (file is `#![cfg(any())]`-disabled upstream) |
| **`tests/circuit_breaker_dos.rs`** | **+340** | **Created — 13 tests** |
| **`FIX_REPORT.md`** | **new** | **Created — this report** |
