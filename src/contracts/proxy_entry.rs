#![allow(missing_docs)]

use crate::contracts::logic;
use crate::contracts::validate_address;
use crate::types::{
    BatchCall, ContractError, DataKey, GuardianEntry, RewardStream, Snapshot, SnapshotMeta, Task,
};
use crate::DEFAULT_WEIGHT_THRESHOLD;
use crate::{circuit_breaker, drips, events, guardian, reputation, storage, task};
use soroban_sdk::{contract, contractimpl, panic_with_error, Address, BytesN, Env, Vec};

/// The main entrypoint for the Vero Core contract.
///
/// Implements all contract features including voting, task registration,
/// reputation management, token locking, and upgrades.
#[contract]
pub struct VeroContract;

#[contractimpl]
impl VeroContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        lock_threshold: i128,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &token)?;

        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            return Err(ContractError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TokenAddress, &token);
        env.storage()
            .instance()
            .set(&DataKey::LockThreshold, &lock_threshold);
        env.storage().instance().set(&DataKey::Paused, &false);

        // Grant Admin role to the deployer/initial admin
        let admin_role_key = DataKey::RoleAssignment(admin.clone(), crate::types::Role::Admin);
        env.storage().instance().set(&admin_role_key, &true);

        crate::migrate::set_version(&env, crate::migrate::CURRENT_VERSION);

        env.storage().instance().extend_ttl(100_000, 100_000);
        events::emit_contract_initialized(&env, &admin);
        Ok(())
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    pub fn toggle_pause(env: Env, admin: Address) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::EmergencyManager)?;
        let current = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        let new_paused = !current;
        env.storage().instance().set(&DataKey::Paused, &new_paused);
        events::emit_pause_toggled(&env, new_paused);
        Ok(())
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::EmergencyManager)?;
        env.storage().instance().set(&DataKey::Paused, &true);
        events::emit_pause_toggled(&env, true);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::EmergencyManager)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        events::emit_pause_toggled(&env, false);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn add_guardian(env: Env, admin: Address, guardian: Address) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &guardian)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::GuardianManager)?;
        guardian::add_guardian(&env, admin.clone(), guardian.clone())?;
        events::emit_guardian_added(&env, &admin, &guardian);
        Ok(())
    }

    pub fn remove_guardian(
        env: Env,
        admin: Address,
        guardian: Address,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &guardian)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::GuardianManager)?;
        guardian::remove_guardian(&env, admin.clone(), guardian.clone())?;
        events::emit_guardian_removed(&env, &admin, &guardian);
        Ok(())
    }

    pub fn is_guardian(env: Env, guardian: Address) -> bool {
        guardian::is_guardian(&env, &guardian)
    }

    pub fn set_reputation(
        env: Env,
        admin: Address,
        guardian: Address,
        score: u64,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &guardian)?;
        circuit_breaker::require_not_paused(&env)?;
        reputation::set_reputation(&env, admin.clone(), guardian.clone(), score)?;
        events::emit_reputation_set(&env, &admin, &guardian, score);
        Ok(())
    }

    pub fn get_reputation(env: Env, guardian: Address) -> Option<u64> {
        reputation::get_reputation(&env, &guardian)
    }

    pub fn calculate_voting_power(env: Env, guardian: Address) -> Option<u64> {
        reputation::calculate_voting_power(&env, &guardian)
    }

    pub fn lock_tokens(env: Env, guardian: Address, amount: i128) -> Result<(), ContractError> {
        validate_address(&env, &guardian)?;
        logic::lock_tokens(&env, guardian, amount)
    }

    pub fn request_unlock(env: Env, guardian: Address) -> Result<(), ContractError> {
        validate_address(&env, &guardian)?;
        logic::request_unlock(&env, guardian)
    }

    pub fn unlock_tokens(env: Env, guardian: Address) -> Result<(), ContractError> {
        validate_address(&env, &guardian)?;
        logic::unlock_tokens(&env, guardian)
    }

    /// Recovers tokens from the contract in emergency situations.
    ///
    /// Note: This function deliberately bypasses the circuit breaker pause gate
    /// (`require_not_paused`), as it serves as the recovery mechanism of last resort
    /// when normal contract operations are halted or paused. Requires the caller
    /// to hold the `EmergencyManager` role.
    pub fn emergency_recover(
        env: Env,
        admin: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &recipient)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::EmergencyManager)?;
        logic::emergency_recover(&env, admin, recipient, amount)
    }

    pub fn resign_guardian(env: Env, guardian: Address) -> Result<(), ContractError> {
        validate_address(&env, &guardian)?;
        logic::resign_guardian(&env, guardian)
    }

    pub fn set_weight_threshold(
        env: Env,
        admin: Address,
        threshold: u64,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::ConfigManager)?;
        env.storage()
            .instance()
            .set(&DataKey::WeightThreshold, &threshold);
        events::emit_threshold_set(&env, &admin, threshold);
        Ok(())
    }

    pub fn get_weight_threshold(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::WeightThreshold)
            .unwrap_or(DEFAULT_WEIGHT_THRESHOLD)
    }

    pub fn set_vault_address(env: Env, admin: Address, vault: Address) {
        if validate_address(&env, &admin).is_err() {
            panic_with_error!(env, ContractError::InvalidAddress);
        }
        if validate_address(&env, &vault).is_err() {
            panic_with_error!(env, ContractError::InvalidAddress);
        }
        circuit_breaker::require_not_paused(&env).unwrap();
        // Use try-catch pattern via unwrap since this function has no Result return
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::ConfigManager)
            .unwrap();
        env.storage().instance().set(&DataKey::VaultAddress, &vault);
        events::emit_vault_set(&env, &admin, &vault);
    }

    pub fn set_fee_bps(env: Env, admin: Address, bps: u32) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::ConfigManager)?;
        if bps > 1000 {
            return Err(ContractError::InvalidConfig);
        }
        env.storage().instance().set(&DataKey::FeeBps, &bps);
        Ok(())
    }

    pub fn set_treasury_address(
        env: Env,
        admin: Address,
        treasury: Address,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &treasury)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::ConfigManager)?;
        env.storage()
            .instance()
            .set(&DataKey::TreasuryAddress, &treasury);
        Ok(())
    }

    pub fn register_task(
        env: Env,
        admin: Address,
        task_id: u64,
        min_votes_required: u32,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::TaskManager)?;
        let task_ids = soroban_sdk::vec![&env, task_id];
        task::register_tasks(&env, admin, task_ids, min_votes_required)
    }

    pub fn cancel_task(env: Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::TaskManager)?;
        task::cancel_task(&env, admin, task_id)
    }

    /// Purge a terminal task (done or cancelled) from contract storage.
    ///
    /// Removes the task struct, its voter list, each individual `Voted` record,
    /// and the task id from the `AllTasks` index. Reduces on-chain state size
    /// and the cost of future `get_snapshot` calls.
    ///
    /// Reverts with `TaskNotFound` if no task exists, `TaskNotTerminal` if the
    /// task is still active, and `NotAuthorized` if the caller is not the admin.
    pub fn purge_task(env: Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::TaskManager)?;
        task::purge_task(&env, admin, task_id)
    }

    pub fn vote(env: Env, guardian: Address, task_id: u64) -> Result<(), ContractError> {
        validate_address(&env, &guardian)?;
        logic::process_vote(&env, guardian, task_id)
    }

    pub fn vote_batch(
        env: Env,
        guardian: Address,
        task_ids: Vec<u64>,
    ) -> Result<(), ContractError> {
        validate_address(&env, &guardian)?;
        logic::process_vote_batch(&env, guardian, task_ids)
    }

    pub fn get_task(env: Env, task_id: u64) -> Option<crate::types::Task> {
        task::get_task(&env, task_id)
    }

    /// Archives a resolved, stale task, moving it from active to archived storage.
    ///
    /// Requires the `TaskManager` role. This was previously permissionless;
    /// however, `start_drips_stream` only resolves tasks from active storage
    /// (no archived-storage fallback), so an unauthorized early archive could
    /// permanently block a task's reward stream from ever starting. Gating
    /// this behind `TaskManager`, consistent with `cancel_task`/`purge_task`,
    /// prevents that griefing vector.
    pub fn archive_task(env: Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::TaskManager)?;
        storage::archive_task(&env, task_id)?;
        events::emit_task_archived(&env, task_id);
        Ok(())
    }

    pub fn get_archived_task(env: Env, task_id: u64) -> Option<crate::types::Task> {
        storage::get_archived_task(&env, task_id)
    }

    pub fn start_reward_stream(
        env: Env,
        admin: Address,
        drips_address: Address,
        contributor: Address,
        task_id: u64,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &drips_address)?;
        validate_address(&env, &contributor)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::TreasuryManager)?;

        let result = drips::start_drips_stream(&env, drips_address, contributor.clone(), task_id);

        match &result {
            Ok(()) => events::emit_reward_stream_started(&env, task_id, &contributor),
            Err(_) => events::emit_reward_stream_failed(&env, task_id, &contributor),
        }

        result
    }

    pub fn get_reward_stream(env: Env, task_id: u64) -> Option<RewardStream> {
        drips::get_reward_stream(&env, task_id)
    }

    /// Report an observed failure to the circuit breaker.
    ///
    /// Reporting stays open to any observer, but every report is now
    /// **authenticated, rate-limited and quota-capped per address**, and the
    /// breaker only auto-pauses once several *independent* reporters agree.
    /// This preserves the "any observer can report" design goal while making it
    /// impossible for a single address to unilaterally pause the contract.
    ///
    /// See [`crate::circuit_breaker`] for the full trust-model decision record.
    ///
    /// # Errors
    /// * `InvalidAddress` — reporter is the zero address or the contract itself.
    /// * `UnauthorizedReporter` — trusted-reporters-only mode is enabled and the
    ///   caller is not a guardian / EmergencyManager / Admin.
    /// * `ReportRateLimited` — the caller reported within the cooldown window.
    /// * `ReporterQuotaExceeded` — the caller exhausted its per-window quota.
    pub fn record_failure(env: Env, reporter: Address) -> Result<(), ContractError> {
        validate_address(&env, &reporter)?;
        circuit_breaker::record_failure(&env, reporter)
    }

    /// Current cumulative failure count for the active breaker window.
    pub fn get_failure_count(env: Env) -> u32 {
        circuit_breaker::failure_count(&env)
    }

    /// Number of reports the given address contributed to the active window.
    pub fn get_reporter_failure_count(env: Env, reporter: Address) -> u32 {
        circuit_breaker::reporter_count(&env, &reporter)
    }

    /// Distinct addresses that have reported failures in the active window.
    pub fn get_failure_reporters(env: Env) -> Vec<Address> {
        circuit_breaker::failure_reporters(&env)
    }

    /// Whether failure reporting is currently restricted to trusted monitors.
    pub fn is_trusted_reporters_only(env: Env) -> bool {
        circuit_breaker::trusted_reporters_only(&env)
    }

    /// Restrict (or re-open) failure reporting to trusted monitors — registered
    /// guardians and `EmergencyManager` / `Admin` role holders.
    ///
    /// Intended as an escape hatch if a Sybil flood of reports is ever observed.
    ///
    /// # Errors
    /// * `NotAuthorized` — caller does not hold the `EmergencyManager` role.
    pub fn set_trusted_reporters_only(
        env: Env,
        admin: Address,
        enabled: bool,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::EmergencyManager)?;
        circuit_breaker::set_trusted_reporters_only(&env, enabled);
        events::emit_trusted_reporters_only_set(&env, &admin, enabled);
        Ok(())
    }

    pub fn reset_circuit_breaker(env: Env, admin: Address) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::EmergencyManager)?;
        circuit_breaker::reset(&env, admin.clone())?;
        events::emit_circuit_breaker_reset(&env, &admin);
        Ok(())
    }

    pub fn get_estimated_cost(_env: Env, op: crate::types::Operation) -> u64 {
        crate::gas::get_estimated_cost(op)
    }

    /// Immediately replace the contract's WASM code. Callable only by the
    /// contract admin.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn upgrade_contract(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        crate::contracts::upgrade::upgrade_contract(env, admin, new_wasm_hash)
    }

    // ─── Multi-sig upgrade management ────────────────────────────────────────
    // Implemented in `crate::contracts::upgrade`; see there for full docs.

    /// Configure the list of authorized upgrade signers and the required quorum.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn set_upgrade_signers(
        env: Env,
        admin: Address,
        signers: Vec<Address>,
        threshold: u32,
    ) -> Result<(), ContractError> {
        crate::contracts::upgrade::set_upgrade_signers(env, admin, signers, threshold)
    }

    /// Returns the currently configured list of authorized upgrade signers.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn get_upgrade_signers(env: Env) -> Vec<Address> {
        crate::contracts::upgrade::get_upgrade_signers(env)
    }

    /// Returns the minimum number of upgrade approvals required (quorum).
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn get_upgrade_threshold(env: Env) -> u32 {
        crate::contracts::upgrade::get_upgrade_threshold(env)
    }

    /// Propose a new upgrade WASM hash as an upgrade signer.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn propose_upgrade(
        env: Env,
        signer: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        crate::contracts::upgrade::propose_upgrade(env, signer, new_wasm_hash)
    }

    /// Approve a pending upgrade as an authorized signer.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn approve_upgrade(env: Env, signer: Address) -> Result<(), ContractError> {
        crate::contracts::upgrade::approve_upgrade(env, signer)
    }

    /// Execute the pending upgrade once the approval quorum is met.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn execute_upgrade(env: Env) -> Result<(), ContractError> {
        crate::contracts::upgrade::execute_upgrade(env)
    }

    /// Cancel a pending upgrade. Only the contract admin may call this.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn cancel_upgrade(env: Env, admin: Address) -> Result<(), ContractError> {
        crate::contracts::upgrade::cancel_upgrade(env, admin)
    }

    /// Builds the full contract snapshot atomically. Reverts with
    /// `SnapshotTooLarge` once any tracked collection (guardians, tasks,
    /// reward streams) exceeds `MAX_SNAPSHOT_COLLECTION_SIZE` — at that point
    /// use `get_snapshot_meta` plus the paginated `*_page` calls instead.
    pub fn get_snapshot(env: Env) -> Result<Snapshot, ContractError> {
        logic::get_snapshot(&env)
    }

    pub fn record_snapshot(env: Env) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        logic::record_snapshot(&env)
    }

    /// O(1) snapshot header (paused/admin/thresholds/addresses) plus the
    /// current guardian/task/reward-stream counts. Always safe to call.
    pub fn get_snapshot_meta(env: Env) -> SnapshotMeta {
        logic::get_snapshot_meta(&env)
    }

    /// Returns a bounded page of guardians (with status + reputation)
    /// starting at `offset`. `limit` is capped server-side regardless of the
    /// value passed in. Reads `O(limit)` entries, not `O(total guardian
    /// count)` — stays cheaply invokable at guardian counts where
    /// `get_snapshot` is capped out entirely.
    pub fn get_guardians_page(env: Env, offset: u32, limit: u32) -> Vec<GuardianEntry> {
        logic::get_guardians_page(&env, offset, limit)
    }

    /// Returns a bounded page of tasks starting at `offset`. Reads `O(limit)`
    /// entries, not `O(total task count)`.
    pub fn get_tasks_page(env: Env, offset: u32, limit: u32) -> Vec<Task> {
        logic::get_tasks_page(&env, offset, limit)
    }

    /// Returns a bounded page of reward streams starting at `offset`.
    pub fn get_reward_streams_page(env: Env, offset: u32, limit: u32) -> Vec<RewardStream> {
        logic::get_reward_streams_page(&env, offset, limit)
    }

    pub fn get_snapshot_history(env: Env) -> soroban_sdk::Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::AllSnapshots)
            .unwrap_or(soroban_sdk::Vec::new(&env))
    }

    pub fn get_snapshot_at(env: Env, timestamp: u64) -> Result<Snapshot, ContractError> {
        env.storage()
            .instance()
            .get(&DataKey::Snapshot(timestamp))
            .ok_or(ContractError::SnapshotNotFound)
    }

    pub fn get_withdrawal_timelock(env: Env, guardian: Address) -> Option<u64> {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawalTimelock(guardian))
    }

    pub fn batch_execute(
        env: Env,
        calls: soroban_sdk::Vec<BatchCall>,
    ) -> Result<(), ContractError> {
        for call in calls.iter() {
            match call {
                BatchCall::RegisterTask(admin, task_id, min_votes_required) => {
                    Self::register_task(env.clone(), admin, task_id, min_votes_required)?
                }
                BatchCall::CancelTask(admin, task_id) => {
                    Self::cancel_task(env.clone(), admin, task_id)?
                }
                BatchCall::Vote(guardian, task_id) => Self::vote(env.clone(), guardian, task_id)?,
                BatchCall::AddGuardian(admin, guardian) => {
                    Self::add_guardian(env.clone(), admin, guardian)?
                }
                BatchCall::RemoveGuardian(admin, guardian) => {
                    Self::remove_guardian(env.clone(), admin, guardian)?
                }
                BatchCall::SetReputation(admin, guardian, score) => {
                    Self::set_reputation(env.clone(), admin, guardian, score)?
                }
                BatchCall::LockTokens(guardian, amount) => {
                    Self::lock_tokens(env.clone(), guardian, amount)?
                }
                BatchCall::RequestUnlock(guardian) => Self::request_unlock(env.clone(), guardian)?,
                BatchCall::UnlockTokens(guardian) => Self::unlock_tokens(env.clone(), guardian)?,
                BatchCall::ResignGuardian(guardian) => {
                    Self::resign_guardian(env.clone(), guardian)?
                }
                BatchCall::SetWeightThreshold(admin, threshold) => {
                    Self::set_weight_threshold(env.clone(), admin, threshold)?
                }
                BatchCall::SetVaultAddress(admin, vault) => {
                    Self::set_vault_address(env.clone(), admin, vault)
                }
                BatchCall::SetUpgradeSigners(admin, signers, threshold) => {
                    Self::set_upgrade_signers(env.clone(), admin, signers, threshold)?
                }
                BatchCall::ProposeUpgrade(signer, hash) => {
                    Self::propose_upgrade(env.clone(), signer, hash)?
                }
                BatchCall::ApproveUpgrade(signer) => Self::approve_upgrade(env.clone(), signer)?,
                BatchCall::ExecuteUpgrade(_signer) => Self::execute_upgrade(env.clone())?,
                BatchCall::CancelUpgrade(admin) => Self::cancel_upgrade(env.clone(), admin)?,
                BatchCall::StartRewardStream(admin, drips, contributor, task_id) => {
                    Self::start_reward_stream(env.clone(), admin, drips, contributor, task_id)?
                }
                BatchCall::TogglePause(admin) => Self::toggle_pause(env.clone(), admin)?,
                BatchCall::Pause(admin) => Self::pause(env.clone(), admin)?,
                BatchCall::Unpause(admin) => Self::unpause(env.clone(), admin)?,
                BatchCall::RecordFailure(reporter) => Self::record_failure(env.clone(), reporter)?,
                BatchCall::ResetCircuitBreaker(admin) => {
                    Self::reset_circuit_breaker(env.clone(), admin)?;
                }
                BatchCall::EmergencyRecover(admin, recipient, amount) => {
                    Self::emergency_recover(env.clone(), admin, recipient, amount)?
                }
                BatchCall::SetFeeBps(admin, bps) => Self::set_fee_bps(env.clone(), admin, bps)?,
                BatchCall::SetTreasuryAddress(admin, treasury) => {
                    Self::set_treasury_address(env.clone(), admin, treasury)?
                }
            }
        }
        Ok(())
    }

    // ─── Role-based access control ──────────────────────────────────────

    /// Grant a role to a target address. Only callable by Admin role holders.
    ///
    /// # Errors
    /// * `NotAuthorized` — Caller does not hold the Admin role.
    pub fn grant_role(
        env: Env,
        caller: Address,
        target: Address,
        role: crate::types::Role,
    ) -> Result<(), ContractError> {
        validate_address(&env, &caller)?;
        validate_address(&env, &target)?;
        crate::contracts::rbac::grant_role_internal(&env, &caller, &target, role)
    }

    /// Revoke a role from a target address. Only callable by Admin role holders.
    ///
    /// # Errors
    /// * `NotAuthorized` — Caller does not hold the Admin role.
    /// * `LastAdminRemovalBlocked` — Cannot revoke the last remaining Admin role.
    pub fn revoke_role(
        env: Env,
        caller: Address,
        target: Address,
        role: crate::types::Role,
    ) -> Result<(), ContractError> {
        validate_address(&env, &caller)?;
        validate_address(&env, &target)?;
        crate::contracts::rbac::revoke_role_internal(&env, &caller, &target, role)
    }

    /// Check whether an address holds a specific role.
    pub fn has_role(env: Env, address: Address, role: crate::types::Role) -> bool {
        crate::contracts::rbac::has_role(&env, &address, role)
    }

    /// Returns the currently recorded storage version.
    pub fn get_storage_version(env: Env) -> u32 {
        crate::migrate::get_version(&env)
    }

    /// Run the storage migration to bring the storage schema to the latest version.
    /// Only contract admin can trigger migration.
    pub fn migrate_storage(env: Env, admin: Address) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::Admin)?;
        crate::migrate::migrate(&env)
    }
}
