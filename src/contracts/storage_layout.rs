// The `contracttype` macro generates associated functions that cannot carry
// doc comments; suppress the lint at file scope so those generated items
// don't break -D clippy::all.
#![allow(missing_docs)]

use soroban_sdk::{contracttype, Address};

use crate::types::Role;

/// Canonical storage key definitions for the Vero contract.
///
/// All contract state is stored under these typed keys in instance storage.
/// This is the single source of truth for `DataKey` — `crate::types` re-exports
/// it via `pub use crate::contracts::storage_layout::DataKey`.
///
/// Each enum variant namespaces a distinct domain of contract state to guarantee
/// key uniqueness and prevent collisions across storage reads/writes.
#[contracttype]
#[derive(Clone, PartialEq, Eq)]
pub enum DataKey {
    /// Guardian configuration and status record for a given address.
    /// Namespaces `Guardian` state per address (`DataKey::Guardian(Address)`).
    Guardian(Address),
    /// Reputation score for a given guardian address.
    /// Namespaces integer reputation metrics per guardian (`DataKey::Reputation(Address)`).
    Reputation(Address),
    /// Global consensus weight threshold required for approvals.
    /// Singleton instance key holding consensus threshold configuration.
    WeightThreshold,
    /// Task record indexed by task ID.
    /// Namespaces complete task records (`DataKey::Task(u64)`).
    Task(u64),
    /// Tracks if a guardian has voted on a specific task ID.
    /// Composite key (`DataKey::Voted(task_id, guardian)`) ensuring one vote per task/guardian pair.
    Voted(u64, Address),
    /// List of voter addresses for a specific task ID.
    /// Namespaces vector of voters who participated in the given task (`DataKey::TaskVoters(u64)`).
    TaskVoters(u64),
    /// Global admin address.
    /// Singleton instance key storing the initial/primary contract administrator address.
    Admin,
    /// Role mapping for a given address and role combination.
    /// Composite key (`DataKey::RoleAssignment(address, role)`) storing boolean role assignment.
    RoleAssignment(Address, Role),
    /// Configured Drips protocol contract address.
    /// Singleton key pointing to external Drips integration contract.
    DripsAddress,
    /// Configured Vault contract address.
    /// Singleton key pointing to the escrow/vault contract.
    VaultAddress,
    /// Reward stream record indexed by task ID.
    /// Namespaces stream configuration and metadata for a given task (`DataKey::RewardStream(u64)`).
    RewardStream(u64),
    /// Configured token address used for rewards/stakes.
    /// Singleton key storing the asset contract identifier.
    TokenAddress,
    /// Lock threshold configuration for stake/locks.
    /// Singleton key storing required locking parameters.
    LockThreshold,
    /// Locked token balance for a specific user address.
    /// Namespaces token lock amounts per user (`DataKey::LockedBalance(Address)`).
    LockedBalance(Address),
    /// Lock configuration status.
    /// Singleton key storing general staking/locking status.
    Lock,
    /// Global failure count tracker for circuit breaker.
    /// Singleton counter tracking failed executions within the current monitoring window.
    FailureCount,
    /// Last ledger sequence at which this address reported a failure (u32).
    /// Backs the per-reporter cooldown of the circuit breaker (`DataKey::LastFailureReport(Address)`).
    LastFailureReport(Address),
    /// Number of failure reports this address has contributed to the current
    /// breaker window (u32). Capped by `MAX_REPORTS_PER_REPORTER` (`DataKey::ReporterFailureCount(Address)`).
    ReporterFailureCount(Address),
    /// `Vec<Address>` — distinct reporters that contributed to the current window.
    /// Singleton key storing list of addresses that submitted failure reports.
    FailureReporters,
    /// bool — when true, only guardians / EmergencyManager / Admin may report failures.
    /// Singleton key controlling circuit breaker report permissions.
    TrustedReportersOnly,
    /// Emergency pause state indicator.
    /// Singleton boolean key indicating if contract state mutations are paused.
    Paused,
    /// Vector/list of all active guardians.
    /// Singleton key holding vector of all enrolled guardian addresses.
    AllGuardians,
    /// Vector/list of all task IDs.
    /// Singleton key holding chronological vector of all created task IDs.
    AllTasks,
    /// Vector/list of all recorded votes.
    /// Singleton key holding global sequence of vote records.
    AllVotes,
    /// Vector/list of all reward stream IDs.
    /// Singleton key holding vector of active/historical reward stream IDs.
    AllRewardStreams,
    /// State snapshot indexed by block/sequence identifier.
    /// Namespaces historical state snapshots (`DataKey::Snapshot(snapshot_id)`).
    Snapshot(u64),
    /// Vector/list of all snapshot identifiers.
    /// Singleton key holding vector of snapshot IDs.
    AllSnapshots,
    /// Active task record indexed by task ID.
    /// Namespaces active task lookup index (`DataKey::ActiveTask(task_id)`).
    ActiveTask(u64),
    /// Archived task record indexed by task ID.
    /// Namespaces archived/completed task lookup index (`DataKey::ArchivedTask(task_id)`).
    ArchivedTask(u64),
    /// Contract initialization flag.
    /// Singleton boolean key indicating if the contract has executed one-time initialization.
    Initialized,
    /// Timelock expiration for withdrawals by address.
    /// Namespaces withdrawal unlock timestamps per user (`DataKey::WithdrawalTimelock(Address)`).
    WithdrawalTimelock(Address),
    /// Configured signers for contract code upgrades.
    /// Singleton key storing authorized multi-sig addresses for contract upgrades.
    UpgradeSigners,
    /// Required approval threshold for contract code upgrades.
    /// Singleton key storing the quorum threshold for code upgrades.
    UpgradeThreshold,
    /// Pending WASM hash proposed for upgrade.
    /// Singleton key holding the proposed WASM hash under review.
    PendingUpgradeWasm,
    /// Recorded approvals for pending contract upgrades.
    /// Singleton key storing vector of guardian/signer approvals for the pending WASM hash.
    PendingUpgradeApprovals,
    /// Storage schema version identifier.
    /// Singleton key storing numeric storage layout version for migration tracking.
    StorageVersion,
    /// Fee percentage basis points configuration.
    /// Singleton key storing protocol fee in basis points (1/100th of 1%).
    FeeBps,
    /// Configured fee recipient treasury address.
    /// Singleton key storing the address destination for collected protocol fees.
    TreasuryAddress,
    /// Dense slot-indexed guardian list (slot -> address), maintained
    /// alongside `AllGuardians` via swap-remove so `get_guardians_page` can
    /// read a bounded page of slots without fetching the whole guardian set.
    /// Namespaces guardian addresses by numeric index (`DataKey::GuardianIndexAt(u32)`).
    GuardianIndexAt(u32),
    /// Reverse lookup (address -> current slot) used to support O(1)
    /// swap-remove from the guardian index on `remove_guardian`.
    /// Namespaces reverse index mapping (`DataKey::GuardianIndexOf(Address)`).
    GuardianIndexOf(Address),
    /// Number of occupied guardian index slots.
    /// Singleton key tracking total count of indexed guardians in dense slot storage.
    GuardianIndexCount,
    /// Dense slot-indexed task list (slot -> task id), maintained alongside
    /// `AllTasks` via swap-remove so `get_tasks_page` can read a bounded
    /// page of slots without fetching the whole task set.
    /// Namespaces task IDs by numeric index (`DataKey::TaskIndexAt(u32)`).
    TaskIndexAt(u32),
    /// Reverse lookup (task id -> current slot) used to support O(1)
    /// swap-remove from the task index on `purge_task`.
    /// Namespaces reverse index mapping (`DataKey::TaskIndexOf(u64)`).
    TaskIndexOf(u64),
    /// Number of occupied task index slots.
    /// Singleton key tracking total count of indexed tasks in dense slot storage.
    TaskIndexCount,
}
