use crate::contracts::logic;
use crate::types::{BatchCall, ContractError, DataKey, RewardStream, Snapshot};
use crate::DEFAULT_WEIGHT_THRESHOLD;
use crate::{circuit_breaker, drips, events, guardian, reputation, storage, task};
use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

#[contract]
/// Public Soroban entrypoint for the Vero core protocol contract.
pub struct VeroContract;

#[contractimpl]
impl VeroContract {
    /// Initialize the contract with its admin, token, and guardian lock threshold.
    ///
    /// May only run once. Stores the initial admin, token address, lock threshold,
    /// and unpaused state, then extends instance storage TTL.
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        lock_threshold: i128,
    ) -> Result<(), ContractError> {
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
        env.storage().instance().extend_ttl(100_000, 100_000);
        Ok(())
    }

    /// Return the configured admin address, if the contract has been initialized.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    /// Flip the paused flag after authenticating the supplied admin address.
    pub fn toggle_pause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        let current = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        env.storage().instance().set(&DataKey::Paused, &!current);
        Ok(())
    }

    /// Pause sensitive protocol actions after authenticating the supplied admin address.
    pub fn pause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    /// Resume sensitive protocol actions after authenticating the supplied admin address.
    pub fn unpause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    /// Return whether sensitive protocol actions are currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Add a guardian to the active guardian registry.
    ///
    /// The contract must not be paused. The admin is authenticated and guardian
    /// address validation is delegated to the guardian module.
    pub fn add_guardian(env: Env, admin: Address, guardian: Address) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        guardian::add_guardian(&env, admin, guardian);
        Ok(())
    }

    /// Remove a guardian from the active guardian registry.
    ///
    /// The contract must not be paused and the admin must authorize the removal.
    pub fn remove_guardian(
        env: Env,
        admin: Address,
        guardian: Address,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        guardian::remove_guardian(&env, admin, guardian);
        Ok(())
    }

    /// Return whether an address is currently registered as a guardian.
    pub fn is_guardian(env: Env, guardian: Address) -> bool {
        guardian::is_guardian(&env, &guardian)
    }

    /// Set a guardian reputation score used for voting-power calculations.
    ///
    /// The contract must not be paused and the admin must authorize the update.
    pub fn set_reputation(
        env: Env,
        admin: Address,
        guardian: Address,
        score: u64,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        reputation::set_reputation(&env, admin, guardian, score);
        Ok(())
    }

    /// Return a guardian reputation score, if one has been configured.
    pub fn get_reputation(env: Env, guardian: Address) -> Option<u64> {
        reputation::get_reputation(&env, &guardian)
    }

    /// Calculate voting power derived from the guardian reputation model.
    pub fn calculate_voting_power(env: Env, guardian: Address) -> Option<u64> {
        reputation::calculate_voting_power(&env, &guardian)
    }

    /// Lock protocol tokens for a guardian before participating in votes.
    pub fn lock_tokens(env: Env, guardian: Address, amount: i128) -> Result<(), ContractError> {
        logic::lock_tokens(&env, guardian, amount)
    }

    /// Start the guardian withdrawal timelock before tokens can be unlocked.
    pub fn request_unlock(env: Env, guardian: Address) -> Result<(), ContractError> {
        logic::request_unlock(&env, guardian)
    }

    /// Unlock a non-guardian's tokens after the withdrawal timelock expires.
    pub fn unlock_tokens(env: Env, guardian: Address) -> Result<(), ContractError> {
        logic::unlock_tokens(&env, guardian)
    }

    /// Remove the caller from the guardian set and unlock tokens after timelock expiry.
    pub fn resign_guardian(env: Env, guardian: Address) -> Result<(), ContractError> {
        logic::resign_guardian(&env, guardian)
    }

    /// Set the task resolution weight threshold after admin authentication.
    pub fn set_weight_threshold(
        env: Env,
        admin: Address,
        threshold: u64,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::WeightThreshold, &threshold);
        Ok(())
    }

    /// Return the configured resolution threshold or the protocol default.
    pub fn get_weight_threshold(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::WeightThreshold)
            .unwrap_or(DEFAULT_WEIGHT_THRESHOLD)
    }

    /// Store the vault contract address used when resolved tasks release funds.
    pub fn set_vault_address(env: Env, admin: Address, vault: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::VaultAddress, &vault);
    }

    /// Register a new task id for guardian voting.
    ///
    /// The contract must not be paused and the supplied admin must match stored
    /// admin state before delegating to the task registration module.
    pub fn register_task(env: Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ContractError::NotAuthorized);
        }
        let task_ids = soroban_sdk::vec![&env, task_id];
        task::register_tasks(&env, admin, task_ids)
    }

    /// Cancel an active task after admin authentication.
    pub fn cancel_task(env: Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
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
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ContractError::NotAuthorized);
        }
        task::purge_task(&env, admin, task_id)
    }

    /// Cast a guardian vote for a single active task.
    pub fn vote(env: Env, guardian: Address, task_id: u64) -> Result<(), ContractError> {
        logic::process_vote(&env, guardian, task_id)
    }

    /// Cast guardian votes for multiple task ids in one atomic batch.
    pub fn vote_batch(
        env: Env,
        guardian: Address,
        task_ids: Vec<u64>,
    ) -> Result<(), ContractError> {
        logic::process_vote_batch(&env, guardian, task_ids)
    }

    /// Return the active task state for a task id, if present.
    pub fn get_task(env: Env, task_id: u64) -> Option<crate::types::Task> {
        task::get_task(&env, task_id)
    }

    /// Move a task from active storage into the archive.
    pub fn archive_task(env: Env, task_id: u64) -> Result<(), ContractError> {
        storage::archive_task(&env, task_id)
    }

    /// Return archived task state for a task id, if present.
    pub fn get_archived_task(env: Env, task_id: u64) -> Option<crate::types::Task> {
        storage::get_archived_task(&env, task_id)
    }

    /// Start a reward stream for a resolved task through the configured drips contract.
    ///
    /// Emits success or failure events around the downstream drips invocation.
    pub fn start_reward_stream(
        env: Env,
        admin: Address,
        drips_address: Address,
        contributor: Address,
        task_id: u64,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        admin.require_auth();

        let result = drips::start_drips_stream(&env, drips_address, contributor.clone(), task_id);

        match &result {
            Ok(()) => events::emit_reward_stream_started(&env, task_id, &contributor),
            Err(_) => events::emit_reward_stream_failed(&env, task_id, &contributor),
        }

        result
    }

    /// Return reward stream metadata for a task id, if a stream is active.
    pub fn get_reward_stream(env: Env, task_id: u64) -> Option<RewardStream> {
        drips::get_reward_stream(&env, task_id)
    }

    /// Record a circuit-breaker failure signal.
    pub fn record_failure(env: Env) {
        circuit_breaker::record_failure(&env);
    }

    /// Reset the circuit breaker after admin authentication.
    pub fn reset_circuit_breaker(env: Env, admin: Address) {
        circuit_breaker::reset(&env, admin);
    }

    /// Estimate gas cost for a supported operation kind.
    pub fn get_estimated_cost(_env: Env, op: crate::types::Operation) -> u64 {
        crate::gas::get_estimated_cost(op)
    }

    /// Upgrade this contract to a new WASM hash after admin authentication.
    pub fn upgrade_contract(env: Env, admin: Address, new_wasm_hash: soroban_sdk::BytesN<32>) {
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    /// Return a current aggregate snapshot of protocol state.
    pub fn get_snapshot(env: Env) -> Snapshot {
        logic::get_snapshot(&env)
    }

    /// Record the current aggregate snapshot in snapshot history.
    pub fn record_snapshot(env: Env) -> Result<(), ContractError> {
        logic::record_snapshot(&env)
    }

    /// Return timestamps for all recorded snapshots.
    pub fn get_snapshot_history(env: Env) -> soroban_sdk::Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::AllSnapshots)
            .unwrap_or(soroban_sdk::Vec::new(&env))
    }

    /// Return a recorded snapshot by timestamp.
    pub fn get_snapshot_at(env: Env, timestamp: u64) -> Result<Snapshot, ContractError> {
        env.storage()
            .instance()
            .get(&DataKey::Snapshot(timestamp))
            .ok_or(ContractError::SnapshotNotFound)
    }

    /// Return the withdrawal timelock ledger timestamp for a guardian, if present.
    pub fn get_withdrawal_timelock(env: Env, guardian: Address) -> Option<u64> {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawalTimelock(guardian))
    }

    /// Execute a sequence of supported protocol calls in order.
    ///
    /// Stops at the first failing call and returns its error.
    pub fn batch_execute(
        env: Env,
        calls: soroban_sdk::Vec<BatchCall>,
    ) -> Result<(), ContractError> {
        for call in calls.iter() {
            match call {
                BatchCall::RegisterTask(admin, task_id) => {
                    Self::register_task(env.clone(), admin, task_id)?
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
                BatchCall::StartRewardStream(admin, drips, contributor, task_id) => {
                    Self::start_reward_stream(env.clone(), admin, drips, contributor, task_id)?
                }
                BatchCall::TogglePause(admin) => Self::toggle_pause(env.clone(), admin)?,
                BatchCall::Pause(admin) => Self::pause(env.clone(), admin)?,
                BatchCall::Unpause(admin) => Self::unpause(env.clone(), admin)?,
                BatchCall::RecordFailure(_admin) => Self::record_failure(env.clone()),
                BatchCall::ResetCircuitBreaker(admin) => {
                    Self::reset_circuit_breaker(env.clone(), admin)
                }
            }
        }
        Ok(())
    }
}
