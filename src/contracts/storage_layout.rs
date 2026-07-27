use soroban_sdk::{contracttype, Address};

use crate::types::Role;

/// Canonical storage key definitions for the Vero contract.
///
/// All contract state is stored under these typed keys in instance storage.
/// This is the single source of truth for `DataKey` — `crate::types` re-exports
/// it via `pub use crate::contracts::storage_layout::DataKey`.
#[contracttype]
#[derive(Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum DataKey {
    Guardian(Address),
    Reputation(Address),
    WeightThreshold,
    Task(u64),
    Voted(u64, Address),
    TaskVoters(u64),
    Admin,
    RoleAssignment(Address, Role),
    DripsAddress,
    VaultAddress,
    RewardStream(u64),
    TokenAddress,
    LockThreshold,
    LockedBalance(Address),
    Lock,
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
    Paused,
    AllGuardians,
    AllTasks,
    AllVotes,
    AllRewardStreams,
    Snapshot(u64),
    AllSnapshots,
    ActiveTask(u64),
    ArchivedTask(u64),
    Initialized,
    WithdrawalTimelock(Address),
    UpgradeSigners,
    UpgradeThreshold,
    PendingUpgradeWasm,
    PendingUpgradeApprovals,
    StorageVersion,
    FeeBps,
    TreasuryAddress,
}
