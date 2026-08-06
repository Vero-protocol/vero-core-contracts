use soroban_sdk::{contracttype, Address};

use crate::types::Role;

/// Canonical storage key definitions for the Vero contract.
///
/// All contract state is stored under these typed keys in instance storage.
/// This is the single source of truth for `DataKey` — `crate::types` re-exports
/// it via `pub use crate::contracts::storage_layout::DataKey`.
#[allow(missing_docs)]
#[contracttype]
#[derive(Clone, PartialEq, Eq)]
pub enum DataKey {
    /// Guardian record for a given address.
    Guardian(Address),
    /// Reputation score for a given guardian address.
    Reputation(Address),
    /// Global consensus weight threshold required for approvals.
    WeightThreshold,
    /// Task record indexed by task ID.
    Task(u64),
    /// Tracks if a guardian has voted on a specific task ID.
    Voted(u64, Address),
    /// List of voter addresses for a specific task ID.
    TaskVoters(u64),
    /// Global admin address.
    Admin,
    /// Role mapping for a given address and role combination.
    RoleAssignment(Address, Role),
    /// Configured Drips protocol contract address.
    DripsAddress,
    /// Configured Vault contract address.
    VaultAddress,
    /// Reward stream record indexed by task ID.
    RewardStream(u64),
    /// Configured token address used for rewards/stakes.
    TokenAddress,
    /// Lock threshold configuration for stake/locks.
    LockThreshold,
    /// Locked token balance for a specific user address.
    LockedBalance(Address),
    /// Lock configuration status.
    Lock,
    /// Global failure count tracker for circuit breaker.
    FailureCount,
    /// Last ledger sequence at which this address reported a failure (u32).
    /// Backs the per-reporter cooldown of the circuit breaker.
    LastFailureReport(Address),
    /// Number of failure reports this address has contributed to the current
    /// breaker window (u32). Capped by `MAX_REPORTS_PER_REPORTER`.
    ReporterFailureCount(Address),
    /// `Vec<Address>` — distinct reporters that contributed to the current window.
    FailureReporters,
    /// bool — when true, only guardians / EmergencyManager / Admin may report failures.
    TrustedReportersOnly,
    /// Emergency pause state indicator.
    Paused,
    /// Vector/list of all active guardians.
    AllGuardians,
    /// Vector/list of all task IDs.
    AllTasks,
    /// Vector/list of all recorded votes.
    AllVotes,
    /// Vector/list of all reward stream IDs.
    AllRewardStreams,
    /// State snapshot indexed by block/sequence identifier.
    Snapshot(u64),
    /// Vector/list of all snapshot identifiers.
    AllSnapshots,
    /// Active task record indexed by task ID.
    ActiveTask(u64),
    /// Archived task record indexed by task ID.
    ArchivedTask(u64),
    /// Contract initialization flag.
    Initialized,
    /// Timelock expiration for withdrawals by address.
    WithdrawalTimelock(Address),
    /// Configured signers for contract code upgrades.
    UpgradeSigners,
    /// Required approval threshold for contract code upgrades.
    UpgradeThreshold,
    /// Pending WASM hash proposed for upgrade.
    PendingUpgradeWasm,
    /// Recorded approvals for pending contract upgrades.
    PendingUpgradeApprovals,
    /// Storage schema version identifier.
    StorageVersion,
    /// Fee percentage basis points configuration.
    FeeBps,
    /// Configured fee recipient treasury address.
    TreasuryAddress,
    /// Dense slot-indexed guardian list (slot -> address), maintained
    /// alongside `AllGuardians` via swap-remove so `get_guardians_page` can
    /// read a bounded page of slots without fetching the whole guardian set.
    GuardianIndexAt(u32),
    /// Reverse lookup (address -> current slot) used to support O(1)
    /// swap-remove from the guardian index on `remove_guardian`.
    GuardianIndexOf(Address),
    /// Number of occupied guardian index slots.
    GuardianIndexCount,
    /// Dense slot-indexed task list (slot -> task id), maintained alongside
    /// `AllTasks` via swap-remove so `get_tasks_page` can read a bounded
    /// page of slots without fetching the whole task set.
    TaskIndexAt(u32),
    /// Reverse lookup (task id -> current slot) used to support O(1)
    /// swap-remove from the task index on `purge_task`.
    TaskIndexOf(u64),
    /// Number of occupied task index slots.
    TaskIndexCount,
}
