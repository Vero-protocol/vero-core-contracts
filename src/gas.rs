use crate::types::Operation;

// ─── Instruction-unit cost constants ──────────────────────────────────────────
//
// Calibrated against Soroban's metering schedule (Stellar Protocol 21):
//   • Base invocation overhead      ~500_000 instructions
//   • Instance storage read          ~50_000 instructions per entry
//   • Instance storage write        ~150_000 instructions per entry
//   • Cross-contract call overhead  ~500_000 instructions (per call)
//   • WASM upgrade (deployer)     ~2_000_000 instructions (platform fixed)
//   • Event emission                 ~30_000 instructions per topic/value
//
// Values are intentionally conservative (slightly above observed minimums) so
// that callers using these estimates as a gas limit are unlikely to run short.
// All figures are in instruction units, which map 1-to-1 to the `fee_per_instruction_increment`
// ledger base-fee calculation used by Stellar's fee schedule.

/// `register_task`: base + reentrancy lock write + has() check + task write + unlock write.
/// `500_000 + 150_000 + 50_000 + 150_000 + 150_000`
pub const COST_REGISTER_TASK: u64 = 1_000_000;

/// `vote`:
///   base + circuit-breaker read + 5 reads (token, threshold, balance, voted, task)
///   + reentrancy lock/unlock (2 writes) + voted write + task write + event emission
///   + conditional cross-contract call to vault
///     500_000 + 5*50_000 + 2*150_000 + 2*150_000 + 2*30_000 + 500_000
///
///   500_000 + 5*50_000 + 2*150_000 + 2*150_000 + 2*30_000 + 500_000
pub const COST_VOTE: u64 = 1_960_000;

/// `add_guardian`: base + circuit-breaker read + guardian write.
/// `500_000 + 50_000 + 150_000`
pub const COST_ADD_GUARDIAN: u64 = 700_000;

/// `set_reputation`: base + circuit-breaker read + reputation write.
/// `500_000 + 50_000 + 150_000`
pub const COST_SET_REPUTATION: u64 = 700_000;

/// `lock_tokens`: base + has() check + token cross-contract transfer + balance read + balance write.
/// `500_000 + 50_000 + 500_000 + 50_000 + 150_000`
pub const COST_LOCK_TOKENS: u64 = 1_250_000;

/// `unlock_tokens`:
///   base + has() check + guardian read + balance read + token transfer + balance write
///     500_000 + 50_000 + 50_000 + 50_000 + 500_000 + 150_000
pub const COST_UNLOCK_TOKENS: u64 = 1_300_000;

/// `resign_guardian`:
///   base + has() check + guardian status write + balance read
///   + conditional token transfer + balance write
///     500_000 + 50_000 + 150_000 + 50_000 + 500_000 + 150_000
///
///   500_000 + 50_000 + 150_000 + 50_000 + 500_000 + 150_000
pub const COST_RESIGN_GUARDIAN: u64 = 1_400_000;

/// `set_weight_threshold`: base + threshold write.
/// `500_000 + 150_000`
pub const COST_SET_WEIGHT_THRESHOLD: u64 = 650_000;

/// `start_reward_stream`:
///   base + circuit-breaker read + task read + stream has() check
///   + cross-contract call to Drips + stream write + event
///     500_000 + 50_000 + 50_000 + 50_000 + 500_000 + 150_000 + 30_000
///
///   500_000 + 50_000 + 50_000 + 50_000 + 500_000 + 150_000 + 30_000
pub const COST_START_REWARD_STREAM: u64 = 1_330_000;

/// `toggle_pause` / `pause` / `unpause`: base + paused read + paused write + event.
/// `500_000 + 50_000 + 150_000 + 30_000`
pub const COST_TOGGLE_PAUSE: u64 = 730_000;

/// `record_failure(reporter)`: base + auth + trusted-mode read + cooldown read
/// + reporter-quota read + reporter index read/write + last-report write
/// + reporter-count write + failure-count read/write + conditional paused write
/// + report event + conditional trip event.
/// `500_000 + 4*50_000 + 5*150_000 + 2*30_000`
pub const COST_RECORD_FAILURE: u64 = 1_510_000;

/// `reset_circuit_breaker`: base + failure-count write + paused remove.
/// `500_000 + 150_000 + 150_000`
pub const COST_RESET_CIRCUIT_BREAKER: u64 = 800_000;

/// `upgrade_contract`: base + WASM deployer overhead (fixed platform cost for new wasm hash write).
/// `500_000 + 2_000_000`
pub const COST_UPGRADE_CONTRACT: u64 = 2_500_000;

/// `record_snapshot`: base + get_snapshot reads + 2 writes (AllSnapshots + Snapshot) + event.
/// `500_000 + 20*50_000 + 2*150_000 + 30_000`
pub const COST_RECORD_SNAPSHOT: u64 = 1_830_000;

/// `purge_task`: base + 2 task reads + AllTasks read + per-voter Voted removes (avg 5) +
/// TaskVoters remove + ActiveTask remove + ArchivedTask remove + AllTasks write + event.
/// `500_000 + 2*50_000 + 50_000 + 5*150_000 + 4*150_000 + 30_000`
pub const COST_PURGE_TASK: u64 = 2_030_000;

/// `vote_batch` (batch size = 5):
///
/// base + circuit-breaker read + auth + 5 reads (token, threshold, balance, weight, weight_threshold)
/// + reentrancy lock/unlock (2 writes)
/// + per-task: voted read, task read, voted write, task write, voter append write, event (×5)
/// + conditional vault calls (×5)
///
/// ~ `500_000 + 50_000 + 5*50_000 + 2*150_000 + 5*(50_000 + 50_000 + 150_000 + 150_000 + 150_000 + 30_000 + 500_000)`
///
/// ~ `500_000 + 50_000 + 250_000 + 300_000 + 5*1_080_000`
pub const COST_VOTE_BATCH: u64 = 6_500_000;

/// `set_upgrade_signers`: base + write (UpgradeSigners) + write (UpgradeThreshold).
/// `500_000 + 150_000 + 150_000`
pub const COST_SET_UPGRADE_SIGNERS: u64 = 800_000;

/// `propose_upgrade`: base + signer check read + pending check read + write (PendingUpgradeWasm) + write (PendingUpgradeApprovals) + event.
/// `500_000 + 50_000 + 50_000 + 150_000 + 150_000 + 30_000`
pub const COST_PROPOSE_UPGRADE: u64 = 930_000;

/// `approve_upgrade`: base + signer check read + pending check read + duplicate check + write (PendingUpgradeApprovals) + event.
/// `500_000 + 50_000 + 50_000 + 50_000 + 150_000 + 30_000`
pub const COST_APPROVE_UPGRADE: u64 = 830_000;

/// `execute_upgrade`: base + threshold read + approvals read + signers read + deployer call (costly) + cleanup writes + event.
/// `500_000 + 50_000 + 50_000 + 50_000 + 2_000_000 + 2*150_000 + 30_000`
pub const COST_EXECUTE_UPGRADE: u64 = 2_980_000;

/// `cancel_upgrade`: base + admin auth + 3 removes + event.
/// `500_000 + 3*150_000 + 30_000`
pub const COST_CANCEL_UPGRADE: u64 = 980_000;

/// `emergency_recover`: base + role read + token read + token transfer + event.
/// `500_000 + 50_000 + 50_000 + 500_000 + 30_000`
pub const COST_EMERGENCY_RECOVER: u64 = 1_130_000;

pub const COST_SET_FEE_BPS: u64 = 650_000;
pub const COST_SET_TREASURY_ADDRESS: u64 = 650_000;

// ─── Public mapping function ───────────────────────────────────────────────────

/// Returns the estimated instruction-unit cost for a given [`Operation`].
///
/// This is a pure constant-time function — it does not read or write any
/// storage, perform any authentication, or make cross-contract calls.
/// Callers can use the returned value to set an appropriate `fee` or
/// `resource_fee` when constructing a Soroban transaction.
///
/// # Notes
/// - Costs are conservative upper bounds calibrated against Soroban Protocol 21
///   metering constants. Actual on-chain costs may be lower.
/// - `Vote` and `UpgradeContract` are the most expensive operations.
/// - Pure view functions (`get_task`, `get_reputation`, etc.) are intentionally
///   excluded — their cost is negligible and bounded by the base invocation fee.
pub fn get_estimated_cost(op: Operation) -> u64 {
    match op {
        Operation::RegisterTask => COST_REGISTER_TASK,
        Operation::Vote => COST_VOTE,
        Operation::AddGuardian => COST_ADD_GUARDIAN,
        Operation::SetReputation => COST_SET_REPUTATION,
        Operation::LockTokens => COST_LOCK_TOKENS,
        Operation::UnlockTokens => COST_UNLOCK_TOKENS,
        Operation::ResignGuardian => COST_RESIGN_GUARDIAN,
        Operation::SetWeightThreshold => COST_SET_WEIGHT_THRESHOLD,
        Operation::StartRewardStream => COST_START_REWARD_STREAM,
        Operation::TogglePause => COST_TOGGLE_PAUSE,
        Operation::RecordFailure => COST_RECORD_FAILURE,
        Operation::ResetCircuitBreaker => COST_RESET_CIRCUIT_BREAKER,
        Operation::UpgradeContract => COST_UPGRADE_CONTRACT,
        Operation::RecordSnapshot => COST_RECORD_SNAPSHOT,
        Operation::PurgeTask => COST_PURGE_TASK,
        Operation::VoteBatch => COST_VOTE_BATCH,
        Operation::SetUpgradeSigners => COST_SET_UPGRADE_SIGNERS,
        Operation::ProposeUpgrade => COST_PROPOSE_UPGRADE,
        Operation::ApproveUpgrade => COST_APPROVE_UPGRADE,
        Operation::ExecuteUpgrade => COST_EXECUTE_UPGRADE,
        Operation::CancelUpgrade => COST_CANCEL_UPGRADE,
        Operation::EmergencyRecover => COST_EMERGENCY_RECOVER,
        Operation::SetFeeBps => COST_SET_FEE_BPS,
        Operation::SetTreasuryAddress => COST_SET_TREASURY_ADDRESS,
    }
}
