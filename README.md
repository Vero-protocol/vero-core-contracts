# Vero Core Contracts

On-chain GitHub PR verification for the Stellar ecosystem. Guardians — trusted off-chain validators — cast weighted votes on registered tasks (pull requests). Once cumulative reputation weight meets a configurable threshold the task is marked done, creating a tamper-proof audit trail on Soroban.

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                         VeroContract                             │
│                                                                  │
│  initialize(token, threshold)                                    │
│  add_guardian(admin, guardian)                                   │
│  register_task(admin, task_id)                                   │
│  vote(guardian, task_id) ──► weight check ──► threshold check   │
│  get_task(task_id) ──► Task { id, votes, is_done, weight }       │
│                                                                  │
│  pause(admin) / unpause(admin) / toggle_pause(admin)            │
│  record_failure(reporter) ──► rate-limited, quorum-gated breaker │
│  reset_circuit_breaker(admin)                                    │
└──────────────────────────────────────┬───────────────────────────┘
                                       │ instance storage
                         ┌─────────────┴──────────────┐
                         │          DataKey            │
                         │  Guardian(Address)          │
                         │  Reputation(Address)        │
                         │  Task(u64)                  │
                         │  Voted(u64, Address)        │
                         │  Paused                     │
                         │  FailureCount               │
                         └─────────────────────────────┘
```

**Flow**

1. Admin calls `initialize` with a token address and lock threshold.
2. Admin registers a GitHub PR as a `Task` with a unique numeric ID.
3. Admin whitelists trusted validator addresses as guardians and assigns reputation scores.
4. Guardians lock tokens above the threshold, then call `vote`.
5. Each vote adds the guardian's reputation weight to `total_weight_accrued`.
6. When `total_weight_accrued >= weight_threshold` (default 300) the task's `is_done` flips to `true`.

---

## Modules

| Module            | Responsibility                                                                                    |
| ----------------- | ------------------------------------------------------------------------------------------------- |
| `types`           | `Task`, `DataKey`, `ContractError`, `RewardStream`                                                |
| `guardian`        | Guardian registry with TTL-extended instance storage                                              |
| `limits`          | Single source of truth for protocol size/limit constants                                          |
| `task`            | Task registration and retrieval                                                                   |
| `reputation`      | Guardian reputation scores and voting power calculation                                           |
| `circuit_breaker` | Emergency halt + DoS-resistant failure reporting: `require_not_paused`, `record_failure`, `reset` |
| `reentrancy`      | Mutex lock/unlock guarding `vote` and `register_task`                                             |
| `drips`           | Cross-contract reward stream initiation via Drips protocol                                        |
| `vault`           | Cross-contract escrow release on task resolution                                                  |
| `events`          | On-chain event emission                                                                           |
| `lib`             | Public contract surface and `vote` orchestration                                                  |


---

## Quick Start

### Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked soroban-cli
```

### Build

```bash
cargo build --target wasm32-unknown-unknown --release
```

### Test

```bash
cargo test
```

---

## Code Snippets

### Initialize the contract

```rust
client.initialize(&admin, &token_address, &100i128); // lock threshold = 100
```

> **Important:** `initialize()` only grants `Role::Admin` to the admin address. Before calling any other role-gated entrypoint you must explicitly grant the required roles to the caller (typically the admin itself in single-operator deployments).

### Grant management roles to the admin

```rust
// Required for add_guardian / remove_guardian / set_reputation
client.grant_role(&admin, &admin, &Role::GuardianManager);

// Required for register_task / cancel_task / purge_task
client.grant_role(&admin, &admin, &Role::TaskManager);

// Required for set_weight_threshold / set_vault_address / set_fee_bps / set_treasury_address
client.grant_role(&admin, &admin, &Role::ConfigManager);

// Required for pause / unpause / toggle_pause / reset_circuit_breaker / emergency_recover
client.grant_role(&admin, &admin, &Role::EmergencyManager);

// Required for start_reward_stream
client.grant_role(&admin, &admin, &Role::TreasuryManager);
```

### Add a guardian and set reputation

```rust
// Requires Role::GuardianManager (granted above)
client.add_guardian(&admin, &validator_address);
client.set_reputation(&admin, &validator_address, &300u64); // score = 300
```

### Lock tokens (guardian must do this before voting)

```rust
client.lock_tokens(&guardian, &150i128); // amount > threshold
```

### Register a task

```rust
// Requires Role::TaskManager (granted above). min_votes_required = minimum guardian votes before resolution.
client.register_task(&admin, &pr_number, &1u32);
```

### Cast a vote

```rust
client.vote(&guardian, &pr_number)?;
```

### Query task state

```rust
let task = client.get_task(&pr_number).unwrap();
assert!(task.is_done); // true once weight threshold is reached
```

---

## Storage Design

All state lives in **instance storage** — scoped to the contract instance and extended with a 100 000-ledger TTL window on every guardian write. Keys are typed via the `DataKey` enum so there are no raw string collisions.

```rust
pub enum DataKey {
    Guardian(Address),             // bool — is this address a guardian?
    Reputation(Address),           // u64 — reputation score
    WeightThreshold,               // u64 — cumulative weight required to resolve
    Task(u64),                     // Task — the live (active) task entry
    Voted(u64, Address),           // bool — has this guardian voted on this task?
    TaskVoters(u64),               // Vec<Address> — guardians who voted on this task
    Admin,                         // Address — the multi-sig admin account
    RoleAssignment(Address, Role), // bool — does this address hold this RBAC role?
    DripsAddress,                  // Address — the Drips protocol contract
    VaultAddress,                  // Address — escrow vault contract
    RewardStream(u64),             // RewardStream — active drip stream for a task
    TokenAddress,                  // Address — locked token contract
    LockThreshold,                 // i128 — minimum locked balance to vote
    LockedBalance(Address),        // i128 — tokens locked by a guardian
    Lock,                          // re-entrancy mutex
    FailureCount,                  // u32 — circuit breaker failure counter
    Paused,                        // bool — emergency halt flag
    AllGuardians,                  // Vec<Address> — index of every registered guardian
    AllTasks,                      // Vec<u64> — index of every task id tracked by the contract
    AllVotes,                      // reserved — superseded by the TaskVoters(u64) index, not currently written
    AllRewardStreams,              // Vec<u64> — index of task ids with an active reward stream
    Snapshot(u64),                 // Snapshot — recorded contract state at a given timestamp
    AllSnapshots,                  // Vec<u64> — index of recorded snapshot timestamps
    ActiveTask(u64),               // reserved — active tasks are stored under Task(u64), not currently written
    ArchivedTask(u64),             // Task — archived copy of a resolved, stale task
    Initialized,                   // bool — has the contract's constructor already run?
    WithdrawalTimelock(Address),   // u64 — timestamp a guardian's token unlock was requested
    UpgradeSigners,                // Vec<Address> — addresses authorized to approve contract upgrades
    UpgradeThreshold,              // u32 — number of signer approvals required to execute an upgrade
    PendingUpgradeWasm,            // BytesN<32> — hash of the WASM proposed for the pending upgrade
    PendingUpgradeApprovals,       // Vec<Address> — signers who have approved the pending upgrade
    StorageVersion,                // u32 — schema version of on-chain storage, used by migrations
    FeeBps,                        // u32 — protocol fee in basis points
    TreasuryAddress,               // Address — destination for collected protocol fees
}
```

---

## Error Codes

| Code | Variant | Meaning |
|---|---|---|
| 1 | `NotAuthorized` | Caller is not a registered guardian or admin |
| 2 | `DuplicateVote` | Guardian already voted on this task |
| 3 | `TaskNotVerified` | Task is not yet resolved; cannot start reward stream |
| 4 | `StreamAlreadyActive` | A reward stream for this task already exists |
| 5 | `DripsCallFailed` | Cross-contract call to Drips protocol reverted |
| 6 | `Locked` | Re-entrancy guard is active |
| 7 | `AlreadyInitialized` | Contract has already been initialized |
| 8 | `NotInitialized` | Contract has not been initialized |
| 9 | `InsufficientLockedBalance` | Guardian's locked balance does not exceed the threshold |
| 10 | `WeightOverflow` | Adding vote weight would overflow u64 |
| 11 | `StillGuardian` | Cannot unlock tokens while still registered as a guardian |
| 12 | `NotGuardian` | Address is not a registered guardian |
| 13 | `ZeroWeightVote` | Guardian's reputation score is zero |
| 14 | `NoReputationScore` | Guardian has no reputation score assigned |
| 15 | `ContractPaused` | Contract is paused; all state-changing calls are blocked |
| 16 | `EscrowUnavailable` | Cross-contract call to vault/escrow reverted |

| 38 | `ReportRateLimited` | Reporter already reported within the cooldown window |
| 39 | `ReporterQuotaExceeded` | Reporter exhausted its per-window failure-report quota |
| 40 | `UnauthorizedReporter` | Trusted-reporters-only mode is on and caller is untrusted |

| 17 | `TaskCancelled` | Task has been cancelled and cannot be processed |
| 18 | `InvalidAddress` | Invalid address provided |
| 19 | `InvalidAmount` | Invalid amount provided |
| 20 | `InvalidConfig` | Invalid configuration |
| 21 | `InvalidRange` | Value is outside valid range |
| 22 | `BatchTooLarge` | Batch operation is too large |
| 23 | `TaskNotFound` | Task not found |
| 24 | `TaskAlreadyArchived` | Task has already been archived |
| 25 | `TaskNotStale` | Task is not stale enough to be pruned |
| 26 | `SnapshotNotFound` | Snapshot not found |
| 27 | `WithdrawalTimelockActive` | Withdrawal timelock is still active |
| 28 | `TaskNotTerminal` | Task is not in terminal state |
| 29 | `InsufficientReputation` | Insufficient reputation score |
| 30 | `NotUpgradeSigner` | Caller is not authorized as a multi-sig upgrade signer |
| 31 | `UpgradeThresholdNotMet` | Not enough upgrade approvals collected yet |
| 32 | `NoPendingUpgrade` | No pending upgrade proposal to act on |
| 33 | `AlreadyApproved` | Signer has already approved this upgrade proposal |
| 34 | `InvalidUpgradeConfig` | Invalid multi-sig upgrade configuration (threshold > signers or zero) |
| 35 | `LastAdminRemovalBlocked` | Cannot revoke the last remaining Admin role holder (would cause lockout) |
| 36 | `DuplicateGuardian` | Attempted to add a guardian that is already registered |
| 37 | `InvalidVersion` | Storage version mismatch during pre-flight checks |



---

## Emergency Halt (Circuit Breaker)

The contract has a two-track emergency halt system that allows an admin to immediately freeze all state-changing operations if a vulnerability is discovered, without requiring a contract migration.

### Manual pause / unpause

```rust
// Immediately block all state-changing entry points
client.pause(&admin);

// Restore normal operation
client.unpause(&admin);

// Or toggle the current state
client.toggle_pause(&admin);

// Check current state
let frozen: bool = client.is_paused();
```

Both `pause` and `unpause` require the caller to hold `Role::EmergencyManager` (granted via `grant_role`). The `admin` address can call them after being granted the EmergencyManager role in the setup steps above.

When paused, any call to `register_task`, `vote`, `add_guardian`, `set_reputation`, `set_weight_threshold`, or `start_reward_stream` returns `Err(ContractError::ContractPaused)` immediately.

### Automatic circuit breaker

Off-chain monitors report observed failures via `record_failure(reporter)`. The
breaker auto-pauses the contract (and emits `cb_trip`) only when **both**
conditions hold:

* more than **50 cumulative failure reports** in the current window, **and**
* at least **3 distinct reporters** contributed to them.

```rust
// Called by an off-chain monitor after observing a failed invocation.
// The reporter address must sign the transaction.
client.record_failure(&monitor_address);
```

Observability helpers:

```rust
client.get_failure_count();                  // u32 — reports in this window
client.get_reporter_failure_count(&monitor); // u32 — reports by this address
client.get_failure_reporters();              // Vec<Address> — distinct reporters
client.is_trusted_reporters_only();          // bool — is gating enabled?
```

To resume after investigation:

```rust
// Resets the counter, clears all per-reporter accounting, and unpauses
client.reset_circuit_breaker(&admin);
```

#### Trust model for `record_failure` (decision record)

**Decision: "permissionless but authenticated, rate-limited, and quorum-gated."**

The original entry point took no arguments, required no auth, and simply bumped
a global counter — so any single address could call it 51 times and freeze every
guardian vote, task registration and token lock at will. Reporting remains open
to any observer (the original design goal), but the trust the README previously
only *implied* is now enforced on-chain by five layered controls:

| # | Control | Effect |
|---|---|---|
| 1 | **Authenticated reporter** — `record_failure(reporter)` + `reporter.require_auth()` | Reports are attributable; anonymous bumps are impossible |
| 2 | **Per-reporter cooldown** — `REPORT_COOLDOWN_LEDGERS = 10` | One report per address per 10 ledgers; a 51-call loop in one txn is rejected after the first call, since the ledger sequence is constant within a transaction |
| 3 | **Per-reporter quota** — `MAX_REPORTS_PER_REPORTER = 5` | One address contributes at most 5 of the 50 required reports per window, no matter how long it waits |
| 4 | **Distinct-reporter quorum** — `MIN_DISTINCT_REPORTERS = 3` | The breaker only trips with corroboration from independent addresses |
| 5 | **Trusted-monitor mode** — `set_trusted_reporters_only(admin, true)` | Escape hatch: an `EmergencyManager` can restrict reporting to registered guardians and Emergency/Admin role holders if a Sybil flood is observed |

Controls 3 and 4 are enforced by a compile-time assertion in
`src/circuit_breaker.rs`:

```
MAX_REPORTS_PER_REPORTER * (MIN_DISTINCT_REPORTERS - 1) <= FAILURE_THRESHOLD
                       5 * 2 = 10                       <= 50
```

so **no single address — and no coalition below the quorum — can ever reach the
threshold.** Manual `pause` remains the instant, role-gated emergency stop; the
breaker is a slow, corroborated safety net, not a lever any observer can pull
alone.

Rejected reports return a typed error: `ReportRateLimited` (38),
`ReporterQuotaExceeded` (39), or `UnauthorizedReporter` (40).

**Alternatives considered.** Restricting `record_failure` to guardians/roles was
rejected because it discards the "any observer can report" goal and concentrates
liveness signalling in the same set that the breaker is meant to protect against.
Requiring a verifiable failed-invocation hash was rejected as unenforceable
on-chain: Soroban cannot independently verify another transaction's outcome, so
the hash would be an unchecked argument that merely raises the attacker's cost
of generating distinct values.

### Emergency halt procedure

1. **Detect** — Either trigger `pause` manually, or wait for corroborated
   `record_failure` reports to trip the breaker (>50 reports from ≥3 reporters).
2. **Verify** — Call `is_paused()` on-chain to confirm the contract is frozen,
   and `get_failure_reporters()` to see who reported.
3. **Investigate** — Audit storage state and transaction history off-chain.
4. **Remediate** — Deploy a patched WASM via `upgrade_contract` if needed. If the
   reports were spam, enable `set_trusted_reporters_only(&admin, &true)`.
5. **Resume** — Call `reset_circuit_breaker` (resets counter, clears per-reporter
   accounting, unpauses) or `unpause` if the failure counter was not the trigger.

> **Security note:** Only `EmergencyManager` role holders can call `pause`,
> `unpause`, `reset_circuit_breaker`, and `set_trusted_reporters_only`. The
> `grant_role` call to assign EmergencyManager itself requires `Role::Admin`,
> ensuring role delegation is strictly controlled. The `record_failure` entry
> point stays open to any observer, but it is authenticated, rate-limited,
> quota-capped and quorum-gated — a single address cannot pause the contract, and
> reports can never manipulate task or guardian state.


---

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for dev environment setup, build/test/lint instructions, branch and PR conventions, and how to find good first issues.

---

## License

[MIT License](LICENSE)



