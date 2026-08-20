#![allow(missing_docs)]

use soroban_sdk::{contracterror, contracttype, Address, BytesN, Map};

pub use crate::contracts::storage_layout::DataKey;

/// Role identifiers used by contract-level access control.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Role {
    Admin = 1,
    GuardianManager = 2,
    TaskManager = 3,
    ConfigManager = 4,
    EmergencyManager = 5,
    TreasuryManager = 6,
}

/// Standard contract error codes.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Error {
    /// Action requires admin authorization.
    NotAdmin = 1,
    /// Action requires guardian authorization.
    NotGuardian = 2,
    /// The task has already been resolved.
    TaskAlreadyResolved = 3,
    /// The voter has already voted on this task.
    DuplicateVote = 4,
}

/// A request to withdraw locked tokens.
#[contracttype]
#[derive(Clone)]
pub struct WithdrawalRequest {
    /// Unique identifier for the withdrawal request.
    pub id: u64,
    /// The recipient address of the withdrawn tokens.
    pub recipient: Address,
    /// The amount of tokens to withdraw.
    pub amount: i128,
    /// The ledger sequence number at which the withdrawal was requested.
    pub requested_at_ledger: u32,
    /// Whether the withdrawal has been executed.
    pub is_executed: bool,
    /// Whether the withdrawal has been cancelled.
    pub is_cancelled: bool,
}

/// A voting task to be resolved by guardians.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    /// Unique identifier for the task.
    pub id: u64,
    /// Total number of votes cast.
    pub votes: u32,
    /// Whether the task has been resolved.
    pub is_done: bool,
    /// Timestamp when the task was resolved.
    pub resolved_at: u64,
    /// Cumulative voting weight accrued from guardian votes.
    pub total_weight_accrued: u64,
    /// Whether the task was cancelled.
    pub is_cancelled: bool,
    pub min_votes_required: u32,
}

/// A stream setup to distribute rewards for completing a task.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewardStream {
    /// The associated task identifier.
    pub task_id: u64,
    /// The contributor address receiving the rewards.
    pub contributor: Address,
    /// The address of the Drips contract.
    pub drips_contract: Address,
    /// Whether the reward stream is active.
    pub active: bool,
}

/// O(1) snapshot header — safe to call regardless of how many guardians,
/// tasks, or reward streams the protocol has accumulated. Use this plus
/// `get_guardians_page` / `get_tasks_page` / `get_reward_streams_page` to
/// reconstruct full state in bounded chunks when the collections are too
/// large for the atomic `get_snapshot`.
#[contracttype]
#[derive(Clone)]
pub struct SnapshotMeta {
    /// Timestamp when the snapshot header was read.
    pub timestamp: u64,
    /// Whether the contract was paused.
    pub paused: bool,
    /// Number of failures recorded in the circuit breaker.
    pub failure_count: u32,
    /// The weight threshold required to resolve a task.
    pub weight_threshold: u64,
    /// The admin address, if set.
    pub admin: Option<Address>,
    /// The vault address, if set.
    pub vault_address: Option<Address>,
    /// The drips contract address, if set.
    pub drips_address: Option<Address>,
    /// Total number of registered guardians.
    pub guardian_count: u32,
    /// Total number of tracked tasks.
    pub task_count: u32,
    /// Total number of tracked reward streams.
    pub reward_stream_count: u32,
}

/// A single page entry returned by `get_guardians_page`.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardianEntry {
    /// The guardian's address.
    pub address: Address,
    /// Whether the address is currently a registered guardian.
    pub is_guardian: bool,
    /// The guardian's reputation score, if one has been set.
    pub reputation: Option<u64>,
}

/// A snapshot of the contract state at a specific point in time.
#[contracttype]
#[derive(Clone)]
pub struct Snapshot {
    /// Timestamp when the snapshot was recorded.
    pub timestamp: u64,
    /// Whether the contract was paused.
    pub paused: bool,
    /// Number of failures recorded in the circuit breaker.
    pub failure_count: u32,
    /// The weight threshold required to resolve a task.
    pub weight_threshold: u64,
    /// The admin address, if set.
    pub admin: Option<Address>,
    /// The vault address, if set.
    pub vault_address: Option<Address>,
    /// The drips contract address, if set.
    pub drips_address: Option<Address>,
    /// Map of registered guardian addresses.
    pub guardians: Map<Address, bool>,
    /// Map of guardian reputation scores.
    pub reputations: Map<Address, u64>,
    /// Map of task structures by their ID.
    pub tasks: Map<u64, Task>,
    /// Map tracking votes by (task_id, guardian_address).
    pub votes: Map<(u64, Address), bool>,
    /// Map of reward streams by task ID.
    pub reward_streams: Map<u64, RewardStream>,
}

/// A single call within a `batch_execute` transaction.
///
/// This is the **batchable subset** of [`Operation`]: every variant maps
/// one-to-one onto an [`Operation`] variant via [`BatchCall::operation`],
/// which is the canonical place to register that mapping. A few operations
/// are intentionally not batchable — immediate `Operation::UpgradeContract`,
/// `Operation::RecordSnapshot`, `Operation::PurgeTask`, and the
/// `Operation::VoteBatch` meta-operation — and therefore have no `BatchCall`
/// counterpart.
#[contracttype]
#[derive(Clone)]
pub enum BatchCall {
    RegisterTask(Address, u64, u32),
    CancelTask(Address, u64),
    Vote(Address, u64),
    AddGuardian(Address, Address),
    RemoveGuardian(Address, Address),
    SetReputation(Address, Address, u64),
    LockTokens(Address, i128),
    RequestUnlock(Address),
    UnlockTokens(Address),
    ResignGuardian(Address),
    SetWeightThreshold(Address, u64),
    SetVaultAddress(Address, Address),
    StartRewardStream(Address, Address, Address, u64),
    TogglePause(Address),
    Pause(Address),
    Unpause(Address),
    RecordFailure(Address),
    ResetCircuitBreaker(Address),
    EmergencyRecover(Address, Address, i128),
    /// Set multi-sig upgrade signers and threshold.
    SetUpgradeSigners(Address, soroban_sdk::Vec<Address>, u32),
    /// Propose a new upgrade WASM hash.
    ProposeUpgrade(Address, BytesN<32>),
    /// Approve a pending upgrade.
    ApproveUpgrade(Address),
    /// Execute the upgrade once threshold is met.
    ExecuteUpgrade(Address),
    /// Cancel a pending upgrade.
    CancelUpgrade(Address),
    SetFeeBps(Address, u32),
    SetTreasuryAddress(Address, Address),
}

impl BatchCall {
    /// Returns the [`Operation`] identifier for this batchable call.
    ///
    /// This is the single source of truth linking the batchable subset to the
    /// canonical [`Operation`] enum. The match is exhaustive, so adding a new
    /// `BatchCall` variant forces an update here — which in turn requires a
    /// matching `Operation` variant and a `gas::get_estimated_cost` arm —
    /// keeping batching and gas estimation in sync by construction.
    pub fn operation(&self) -> Operation {
        match self {
            BatchCall::RegisterTask(..) => Operation::RegisterTask,
            BatchCall::CancelTask(..) => Operation::CancelTask,
            BatchCall::Vote(..) => Operation::Vote,
            BatchCall::AddGuardian(..) => Operation::AddGuardian,
            BatchCall::RemoveGuardian(..) => Operation::RemoveGuardian,
            BatchCall::SetReputation(..) => Operation::SetReputation,
            BatchCall::LockTokens(..) => Operation::LockTokens,
            BatchCall::RequestUnlock(..) => Operation::RequestUnlock,
            BatchCall::UnlockTokens(..) => Operation::UnlockTokens,
            BatchCall::ResignGuardian(..) => Operation::ResignGuardian,
            BatchCall::SetWeightThreshold(..) => Operation::SetWeightThreshold,
            BatchCall::SetVaultAddress(..) => Operation::SetVaultAddress,
            BatchCall::StartRewardStream(..) => Operation::StartRewardStream,
            BatchCall::TogglePause(..) => Operation::TogglePause,
            BatchCall::Pause(..) => Operation::Pause,
            BatchCall::Unpause(..) => Operation::Unpause,
            BatchCall::RecordFailure(..) => Operation::RecordFailure,
            BatchCall::ResetCircuitBreaker(..) => Operation::ResetCircuitBreaker,
            BatchCall::EmergencyRecover(..) => Operation::EmergencyRecover,
            BatchCall::SetUpgradeSigners(..) => Operation::SetUpgradeSigners,
            BatchCall::ProposeUpgrade(..) => Operation::ProposeUpgrade,
            BatchCall::ApproveUpgrade(..) => Operation::ApproveUpgrade,
            BatchCall::ExecuteUpgrade(..) => Operation::ExecuteUpgrade,
            BatchCall::CancelUpgrade(..) => Operation::CancelUpgrade,
            BatchCall::SetFeeBps(..) => Operation::SetFeeBps,
            BatchCall::SetTreasuryAddress(..) => Operation::SetTreasuryAddress,
        }
    }
}

/// Every public write operation exposed by `VeroContract`.
///
/// This is the canonical, single source of truth for the contract's write
/// surface. [`BatchCall`] is its batchable subset: every `BatchCall` variant
/// maps to exactly one `Operation` variant (see [`BatchCall::operation`]), so
/// `gas::get_estimated_cost` has an entry for every batchable operation.
///
/// A handful of operations are intentionally non-batchable and therefore have
/// no [`BatchCall`] counterpart: `UpgradeContract` (immediate, admin-only WASM
/// replacement), `RecordSnapshot`, `PurgeTask` (bulk storage removal), and
/// `VoteBatch` (itself a batch meta-operation).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    RegisterTask = 0,
    Vote = 1,
    AddGuardian = 2,
    SetReputation = 3,
    LockTokens = 4,
    UnlockTokens = 5,
    ResignGuardian = 6,
    SetWeightThreshold = 7,
    StartRewardStream = 8,
    TogglePause = 9,
    RecordFailure = 10,
    ResetCircuitBreaker = 11,
    UpgradeContract = 12,
    RecordSnapshot = 13,
    PurgeTask = 14,
    /// `vote_batch` — vote on multiple tasks in one transaction.
    VoteBatch = 15,
    /// `set_upgrade_signers` — configure multi-sig upgrade signers.
    SetUpgradeSigners = 16,
    /// `propose_upgrade` — propose a new upgrade WASM hash.
    ProposeUpgrade = 17,
    /// `approve_upgrade` — approve a pending upgrade.
    ApproveUpgrade = 18,
    /// `execute_upgrade` — execute upgrade once threshold met.
    ExecuteUpgrade = 19,
    /// `cancel_upgrade` — cancel a pending upgrade.
    CancelUpgrade = 20,
    /// `emergency_recover` — emergency token recovery while normal flows are unavailable.
    EmergencyRecover = 21,
    SetFeeBps = 22,
    SetTreasuryAddress = 23,
    /// `cancel_task` — cancel an active task.
    CancelTask = 24,
    /// `remove_guardian` — remove a guardian and its reputation/index entries.
    RemoveGuardian = 25,
    /// `request_unlock` — initiate a timelocked token unlock.
    RequestUnlock = 26,
    /// `set_vault_address` — point the contract at its vault.
    SetVaultAddress = 27,
    /// `pause` — pause the contract.
    Pause = 28,
    /// `unpause` — resume the contract.
    Unpause = 29,
}

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    NotAuthorized = 1,
    DuplicateVote = 2,
    TaskNotVerified = 3,
    StreamAlreadyActive = 4,
    DripsCallFailed = 5,
    Locked = 6,
    AlreadyInitialized = 7,
    NotInitialized = 8,
    InsufficientLockedBalance = 9,
    StillGuardian = 10,
    NotGuardian = 11,
    NoReputationScore = 12,
    ZeroWeightVote = 13,
    WeightOverflow = 14,
    ContractPaused = 15,
    EscrowUnavailable = 16,
    TaskCancelled = 17,
    InvalidAddress = 18,
    InvalidAmount = 19,
    InvalidConfig = 20,
    InvalidRange = 21,
    BatchTooLarge = 22,
    TaskNotFound = 23,
    TaskAlreadyArchived = 24,
    TaskNotStale = 25,
    SnapshotNotFound = 26,
    WithdrawalTimelockActive = 27,
    TaskNotTerminal = 28,
    InsufficientReputation = 29,
    /// Caller is not authorized as a multi-sig upgrade signer.
    NotUpgradeSigner = 30,
    /// Not enough upgrade approvals collected yet.
    UpgradeThresholdNotMet = 31,
    /// No pending upgrade proposal to act on.
    NoPendingUpgrade = 32,
    /// Signer has already approved this upgrade proposal.
    AlreadyApproved = 33,
    /// Invalid multi-sig upgrade configuration (threshold > signers or zero).
    InvalidUpgradeConfig = 34,
    /// Cannot revoke the last remaining Admin role holder (would cause lockout).
    LastAdminRemovalBlocked = 35,
    /// Attempted to add a guardian that is already registered.
    DuplicateGuardian = 36,
    /// Storage version mismatch during pre-flight checks.
    InvalidVersion = 37,

    /// The atomic `get_snapshot`/`record_snapshot` refused to run because a
    /// tracked collection (guardians, tasks, or reward streams) exceeds
    /// `MAX_SNAPSHOT_COLLECTION_SIZE`. Use the paginated snapshot API
    /// (`get_snapshot_meta`, `get_guardians_page`, `get_tasks_page`,
    /// `get_reward_streams_page`) instead.
    SnapshotTooLarge = 38,

    /// Failure report rejected: the caller already reported within the
    /// per-address cooldown window (`REPORT_COOLDOWN_LEDGERS`).
    ReportRateLimited = 39,
    /// Failure report rejected: the caller has already contributed the maximum
    /// number of reports (`MAX_REPORTS_PER_REPORTER`) for the current breaker window.
    ReporterQuotaExceeded = 40,
    /// Failure report rejected: the contract is in "trusted reporters only" mode
    /// and the caller is neither a guardian nor an EmergencyManager/Admin.
    UnauthorizedReporter = 41,
}

#[cfg(test)]
mod tests {
    use super::{BatchCall, Operation};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    #[test]
    fn batch_call_maps_to_operation() {
        let env = Env::default();
        let a = Address::generate(&env);
        let b = Address::generate(&env);

        // Previously-divergent batchable variants now map onto `Operation`.
        assert_eq!(
            BatchCall::CancelTask(a.clone(), 1).operation(),
            Operation::CancelTask
        );
        assert_eq!(
            BatchCall::RemoveGuardian(a.clone(), b.clone()).operation(),
            Operation::RemoveGuardian
        );
        assert_eq!(
            BatchCall::RequestUnlock(a.clone()).operation(),
            Operation::RequestUnlock
        );
        assert_eq!(
            BatchCall::SetVaultAddress(a.clone(), b.clone()).operation(),
            Operation::SetVaultAddress
        );
        assert_eq!(BatchCall::Pause(a.clone()).operation(), Operation::Pause);
        assert_eq!(
            BatchCall::Unpause(a.clone()).operation(),
            Operation::Unpause
        );

        // Spot-check a couple of pre-existing mappings.
        assert_eq!(
            BatchCall::RegisterTask(a.clone(), 1, 1).operation(),
            Operation::RegisterTask
        );
        assert_eq!(BatchCall::Vote(a.clone(), 1).operation(), Operation::Vote);
    }
}
