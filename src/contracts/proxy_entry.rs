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

fn is_strictly_sorted_addresses(addrs: &Vec<Address>) -> bool {
    if addrs.len() < 2 {
        return true;
    }

    let mut prev = addrs.get(0).unwrap();
    for i in 1..addrs.len() {
        let current = addrs.get(i).unwrap();
        if current <= prev {
            return false;
        }
        prev = current;
    }
    true
}

#[contractimpl]
impl VeroContract {
    /// Initializes the Vero Core contract with required admin, token address, and lock threshold.
    ///
    /// Can only be called once when the contract is uninitialized. Sets default weight threshold
    /// and initializes storage version to 1.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Administrator address with top-level protocol authority.
    /// * `token` - Address of the underlying payment/governance token.
    /// * `lock_threshold` - Minimum token amount required for guardian locking.
    ///
    /// # Errors
    /// * [`ContractError::AlreadyInitialized`] - If the contract has already been initialized.
    /// * [`ContractError::InvalidAmount`] - If `lock_threshold` is negative.
    ///
    /// # Side Effects
    /// * Writes initial admin, token, lock threshold, default weight threshold, and storage version to storage.
    /// * Emits `ContractInitialized` event.
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        lock_threshold: i128,
    ) -> Result<(), ContractError> {
        if storage::get_admin(&env).is_some() {
            return Err(ContractError::AlreadyInitialized);
        }
        if lock_threshold < 0 {
            return Err(ContractError::InvalidAmount);
        }

        validate_address(&admin);
        validate_address(&token);

        admin.require_auth();

        storage::set_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::TokenAddress, &token);
        env.storage()
            .instance()
            .set(&DataKey::LockThreshold, &lock_threshold);
        storage::set_weight_threshold(&env, DEFAULT_WEIGHT_THRESHOLD);
        storage::set_storage_version(&env, 1);

        events::emit_contract_initialized(&env, admin, token);

        Ok(())
    }

    /// Queries the current administrator address of the contract.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    ///
    /// # Returns
    /// * `Some(Address)` if initialized, or `None` if uninitialized.
    pub fn get_admin(env: Env) -> Option<Address> {
        storage::get_admin(&env)
    }

    /// Toggles the emergency pause state of the contract (paused <-> unpaused).
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address executing the toggle.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller does not have the admin role or address.
    ///
    /// # Side Effects
    /// * Inverts the pause boolean in storage.
    /// * Emits `ContractPaused` or `ContractUnpaused` event.
    pub fn toggle_pause(env: Env, admin: Address) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        if circuit_breaker::is_paused(&env) {
            circuit_breaker::unpause(&env);
            events::emit_contract_unpaused(&env, admin);
        } else {
            circuit_breaker::pause(&env);
            events::emit_contract_paused(&env, admin);
        }
        Ok(())
    }

    /// Explicitly pauses the contract, disabling regular voting and token locking actions.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address executing the pause.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not authorized as admin.
    ///
    /// # Side Effects
    /// * Sets paused flag in storage and emits `ContractPaused` event.
    pub fn pause(env: Env, admin: Address) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        circuit_breaker::pause(&env);
        events::emit_contract_paused(&env, admin);
        Ok(())
    }

    /// Explicitly unpauses the contract, resuming normal operation.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address executing the unpause.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not authorized as admin.
    ///
    /// # Side Effects
    /// * Clears paused flag in storage and emits `ContractUnpaused` event.
    pub fn unpause(env: Env, admin: Address) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        circuit_breaker::unpause(&env);
        events::emit_contract_unpaused(&env, admin);
        Ok(())
    }

    /// Returns `true` if the contract is currently in an emergency paused state.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    pub fn is_paused(env: Env) -> bool {
        circuit_breaker::is_paused(&env)
    }

    /// Registers a new guardian address and assigns initial default reputation.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address adding the guardian.
    /// * `guardian` - Address of the guardian to register.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    /// * [`ContractError::GuardianAlreadyExists`] - If guardian is already registered.
    ///
    /// # Side Effects
    /// * Registers guardian, initial reputation (1), increments count and appends to guardians list.
    /// * Emits `GuardianAdded` event.
    pub fn add_guardian(env: Env, admin: Address, guardian: Address) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        guardian::add_guardian(&env, &guardian)?;
        reputation::set_reputation(&env, &guardian, 1)?;
        events::emit_guardian_added(&env, guardian);
        Ok(())
    }

    /// Removes an existing guardian address and revokes their reputation.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address removing the guardian.
    /// * `guardian` - Address of the guardian to remove.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    /// * [`ContractError::GuardianNotFound`] - If guardian is not registered.
    ///
    /// # Side Effects
    /// * Removes guardian registration, clears reputation, decrements count and updates list.
    /// * Emits `GuardianRemoved` event.
    pub fn remove_guardian(
        env: Env,
        admin: Address,
        guardian: Address,
    ) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        guardian::remove_guardian(&env, &guardian)?;
        reputation::clear_reputation(&env, &guardian);
        events::emit_guardian_removed(&env, guardian);
        Ok(())
    }

    /// Returns `true` if the given address is currently a registered guardian.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `guardian` - Address to check.
    pub fn is_guardian(env: Env, guardian: Address) -> bool {
        guardian::is_guardian(&env, &guardian)
    }

    /// Sets the reputation score for a registered guardian.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address updating the score.
    /// * `guardian` - Registered guardian address.
    /// * `score` - New reputation score to assign.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    /// * [`ContractError::GuardianNotFound`] - If target address is not a guardian.
    ///
    /// # Side Effects
    /// * Updates reputation in storage and emits `ReputationUpdated` event.
    pub fn set_reputation(
        env: Env,
        admin: Address,
        guardian: Address,
        score: u64,
    ) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        reputation::set_reputation(&env, &guardian, score)?;
        events::emit_reputation_updated(&env, guardian, score);
        Ok(())
    }

    /// Queries the current reputation score for a guardian.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `guardian` - Address being queried.
    ///
    /// # Returns
    /// * `Some(u64)` with reputation score, or `None` if guardian not found.
    pub fn get_reputation(env: Env, guardian: Address) -> Option<u64> {
        reputation::get_reputation(&env, &guardian)
    }

    /// Calculates the weighted voting power for a registered guardian based on reputation.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `guardian` - Address being evaluated.
    ///
    /// # Returns
    /// * `Some(u64)` with voting power, or `None` if not registered or reputation is missing.
    pub fn calculate_voting_power(env: Env, guardian: Address) -> Option<u64> {
        reputation::calculate_voting_power(&env, &guardian)
    }

    /// Locks governance/staking tokens for a guardian.
    ///
    /// Delegates to [`logic::lock_tokens`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `guardian` - Guardian address locking tokens.
    /// * `amount` - Amount of tokens to lock.
    pub fn lock_tokens(env: Env, guardian: Address, amount: i128) -> Result<(), ContractError> {
        logic::lock_tokens(&env, guardian, amount)
    }

    /// Initiates a 24-hour withdrawal timelock for unlocking tokens.
    ///
    /// Delegates to [`logic::request_unlock`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `guardian` - Guardian address requesting unlock.
    pub fn request_unlock(env: Env, guardian: Address) -> Result<(), ContractError> {
        logic::request_unlock(&env, guardian)
    }

    /// Completes token withdrawal after the 24-hour timelock has elapsed.
    ///
    /// Delegates to [`logic::unlock_tokens`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `guardian` - Guardian address unlocking tokens.
    pub fn unlock_tokens(env: Env, guardian: Address) -> Result<(), ContractError> {
        logic::unlock_tokens(&env, guardian)
    }

    /// Recovers contract funds in an emergency when paused.
    ///
    /// Delegates to [`logic::emergency_recover`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address executing the recovery.
    /// * `recipient` - Destination address to receive funds.
    /// * `amount` - Token amount to transfer.
    pub fn emergency_recover(
        env: Env,
        admin: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<(), ContractError> {
        logic::emergency_recover(&env, admin, recipient, amount)
    }

    /// Resigns caller from guardian status after timelock expiration.
    ///
    /// Delegates to [`logic::resign_guardian`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `guardian` - Resigning guardian address.
    pub fn resign_guardian(env: Env, guardian: Address) -> Result<(), ContractError> {
        logic::resign_guardian(&env, guardian)
    }

    /// Sets the minimum total weighted vote threshold needed to resolve tasks.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address updating threshold.
    /// * `threshold` - New weight threshold (must be > 0).
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    /// * [`ContractError::InvalidWeightThreshold`] - If threshold is 0.
    ///
    /// # Side Effects
    /// * Stores new weight threshold and emits `WeightThresholdUpdated` event.
    pub fn set_weight_threshold(
        env: Env,
        admin: Address,
        threshold: u64,
    ) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        if threshold == 0 {
            return Err(ContractError::InvalidWeightThreshold);
        }
        storage::set_weight_threshold(&env, threshold);
        events::emit_weight_threshold_updated(&env, threshold);
        Ok(())
    }

    /// Queries the current weight threshold required for task resolution.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    pub fn get_weight_threshold(env: Env) -> u64 {
        storage::get_weight_threshold(&env)
    }

    /// Configures the escrow/payout vault contract address.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address setting the vault.
    /// * `vault` - Address of the vault contract.
    ///
    /// # Side Effects
    /// * Sets vault address in storage and emits `VaultAddressUpdated` event.
    pub fn set_vault_address(env: Env, admin: Address, vault: Address) {
        if storage::require_admin(&env, &admin).is_err() {
            panic_with_error!(&env, ContractError::Unauthorized);
        }
        validate_address(&vault);
        storage::set_vault_address(&env, &vault);
        events::emit_vault_address_updated(&env, vault);
    }

    /// Configures protocol fee in basis points (max 1000 bps = 10%).
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address setting the fee.
    /// * `bps` - Basis points fee (0 - 1000).
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    /// * [`ContractError::InvalidFeeBps`] - If `bps` > 1000.
    ///
    /// # Side Effects
    /// * Sets fee bps in storage and emits `FeeBpsUpdated` event.
    pub fn set_fee_bps(env: Env, admin: Address, bps: u32) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        storage::set_fee_bps(&env, bps)?;
        events::emit_fee_bps_updated(&env, bps);
        Ok(())
    }

    /// Configures the protocol treasury address to receive collected fees.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address setting the treasury.
    /// * `treasury` - Address to receive protocol fees.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    ///
    /// # Side Effects
    /// * Sets treasury address in storage and emits `TreasuryAddressUpdated` event.
    pub fn set_treasury_address(
        env: Env,
        admin: Address,
        treasury: Address,
    ) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        validate_address(&treasury);
        storage::set_treasury_address(&env, &treasury);
        events::emit_treasury_address_updated(&env, treasury);
        Ok(())
    }

    /// Registers a new PR verification task in the contract.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address registering the task.
    /// * `task_id` - Unique identifier of the task (must be non-zero).
    /// * `min_votes_required` - Minimum number of guardian votes required (must be > 0).
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    /// * [`ContractError::InvalidTaskId`] - If `task_id` is 0.
    /// * [`ContractError::InvalidMinVotes`] - If `min_votes_required` is 0.
    /// * [`ContractError::TaskAlreadyExists`] - If task with `task_id` already exists.
    ///
    /// # Side Effects
    /// * Creates task record in storage and appends ID to tasks list.
    /// * Emits `TaskRegistered` event.
    pub fn register_task(
        env: Env,
        admin: Address,
        task_id: u64,
        min_votes_required: u32,
    ) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        task::register_task(&env, task_id, min_votes_required)?;
        events::emit_task_registered(&env, task_id, min_votes_required);
        Ok(())
    }

    /// Cancels an active unresolved task.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address cancelling the task.
    /// * `task_id` - Identifier of the task to cancel.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    /// * [`ContractError::TaskNotFound`] - If task does not exist.
    /// * [`ContractError::TaskAlreadyResolved`] - If task is already resolved.
    ///
    /// # Side Effects
    /// * Removes task from storage and tasks list.
    /// * Emits `TaskCancelled` event.
    pub fn cancel_task(env: Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        task::cancel_task(&env, task_id)?;
        events::emit_task_cancelled(&env, task_id);
        Ok(())
    }

    /// Completely purges a task and all associated vote storage entries.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address purging the task.
    /// * `task_id` - Identifier of the task to purge.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    /// * [`ContractError::TaskNotFound`] - If task does not exist.
    ///
    /// # Side Effects
    /// * Removes task record, voter map entries, and removes ID from task list.
    /// * Emits `TaskPurged` event.
    pub fn purge_task(env: Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        task::purge_task(&env, task_id)?;
        events::emit_task_purged(&env, task_id);
        Ok(())
    }

    /// Casts a guardian vote for a specific task.
    ///
    /// Delegates to [`logic::process_vote`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `guardian` - Voting guardian address.
    /// * `task_id` - Identifier of the task.
    pub fn vote(env: Env, guardian: Address, task_id: u64) -> Result<(), ContractError> {
        logic::process_vote(&env, guardian, task_id)
    }

    /// Casts guardian votes across a batch of task IDs.
    ///
    /// Delegates to [`logic::process_vote_batch`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `guardian` - Voting guardian address.
    /// * `task_ids` - Vector of task IDs.
    pub fn vote_batch(
        env: Env,
        guardian: Address,
        task_ids: Vec<u64>,
    ) -> Result<(), ContractError> {
        logic::process_vote_batch(&env, guardian, task_ids)
    }

    /// Queries the details of an active registered task.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `task_id` - Identifier of the task.
    ///
    /// # Returns
    /// * `Some(Task)` if found, or `None` if not found.
    pub fn get_task(env: Env, task_id: u64) -> Option<crate::types::Task> {
        task::get_task(&env, task_id)
    }

    /// Moves a completed or resolved task from active storage into the archive.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address archiving the task.
    /// * `task_id` - Identifier of the task to archive.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    /// * [`ContractError::TaskNotFound`] - If task does not exist.
    /// * [`ContractError::TaskNotResolved`] - If task has not yet been resolved.
    ///
    /// # Side Effects
    /// * Moves task data from active key to archive key and removes from tasks list.
    /// * Emits `TaskArchived` event.
    pub fn archive_task(env: Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        task::archive_task(&env, task_id)?;
        events::emit_task_archived(&env, task_id);
        Ok(())
    }

    /// Queries the details of an archived task.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `task_id` - Identifier of the archived task.
    ///
    /// # Returns
    /// * `Some(Task)` if found in archive, or `None` otherwise.
    pub fn get_archived_task(env: Env, task_id: u64) -> Option<crate::types::Task> {
        task::get_archived_task(&env, task_id)
    }

    /// Registers and initiates a Drips streaming reward configuration for a task.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address setting the reward stream.
    /// * `drips_address` - Address of the external Drips contract.
    /// * `contributor` - Address of the reward recipient contributor.
    /// * `task_id` - Identifier of the task.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    /// * [`ContractError::TaskNotFound`] - If task is not registered.
    /// * [`ContractError::StreamAlreadyExists`] - If a stream is already attached to this task.
    ///
    /// # Side Effects
    /// * Stores reward stream struct and appends task ID to reward streams list.
    /// * Emits `RewardStreamStarted` event.
    pub fn start_reward_stream(
        env: Env,
        admin: Address,
        drips_address: Address,
        contributor: Address,
        task_id: u64,
    ) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        validate_address(&drips_address);
        validate_address(&contributor);

        drips::start_reward_stream(&env, drips_address, contributor, task_id)
    }

    /// Queries the Drips reward stream associated with a task ID.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `task_id` - Identifier of the task.
    ///
    /// # Returns
    /// * `Some(RewardStream)` if configured, or `None` otherwise.
    pub fn get_reward_stream(env: Env, task_id: u64) -> Option<RewardStream> {
        drips::get_reward_stream(&env, task_id)
    }

    /// Records an execution failure reported by a trusted reporter or caller.
    ///
    /// Increments failure counters and triggers the circuit breaker pause if failure limit is reached.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `reporter` - Address of the reporting entity.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If trusted reporters mode is enabled and reporter is not authorized.
    ///
    /// # Side Effects
    /// * Increments failure count, appends reporter, and may auto-pause contract.
    /// * Emits `FailureRecorded` and potentially `CircuitBreakerTriggered` event.
    pub fn record_failure(env: Env, reporter: Address) -> Result<(), ContractError> {
        circuit_breaker::record_failure(&env, reporter)
    }

    /// Returns the total cumulative count of recorded failures.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    pub fn get_failure_count(env: Env) -> u32 {
        circuit_breaker::get_failure_count(&env)
    }

    /// Returns the total failure count reported by a specific address.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `reporter` - Address being queried.
    pub fn get_reporter_failure_count(env: Env, reporter: Address) -> u32 {
        circuit_breaker::get_reporter_failure_count(&env, &reporter)
    }

    /// Returns the list of all unique addresses that have reported failures.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    pub fn get_failure_reporters(env: Env) -> Vec<Address> {
        circuit_breaker::get_failure_reporters(&env)
    }

    /// Returns `true` if failure reporting is restricted exclusively to trusted reporters.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    pub fn is_trusted_reporters_only(env: Env) -> bool {
        circuit_breaker::is_trusted_reporters_only(&env)
    }

    /// Configures whether failure reporting is restricted only to trusted reporters.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address modifying setting.
    /// * `enabled` - Boolean flag enabling or disabling restriction.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    ///
    /// # Side Effects
    /// * Updates trusted reporters mode in storage and emits `TrustedReportersModeUpdated` event.
    pub fn set_trusted_reporters_only(
        env: Env,
        admin: Address,
        enabled: bool,
    ) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        circuit_breaker::set_trusted_reporters_only(&env, enabled);
        events::emit_trusted_reporters_mode_updated(&env, enabled);
        Ok(())
    }

    /// Resets the circuit breaker failure counter back to zero and unpauses contract.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address resetting breaker.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    ///
    /// # Side Effects
    /// * Resets failure count to 0 and clears paused state.
    /// * Emits `CircuitBreakerReset` event.
    pub fn reset_circuit_breaker(env: Env, admin: Address) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        circuit_breaker::reset_circuit_breaker(&env);
        events::emit_circuit_breaker_reset(&env, admin);
        Ok(())
    }

    /// Returns estimated compute/gas costs for standard protocol operations.
    ///
    /// # Arguments
    /// * `_env` - Soroban environment.
    /// * `op` - The [`crate::types::Operation`] variant to estimate.
    pub fn get_estimated_cost(_env: Env, op: crate::types::Operation) -> u64 {
        match op {
            crate::types::Operation::Vote => 25_000,
            crate::types::Operation::BatchVote => 80_000,
            crate::types::Operation::RegisterTask => 30_000,
            crate::types::Operation::SetReputation => 20_000,
        }
    }

    /// Directly upgrades the contract WASM executable code (single-admin path).
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address authorizing upgrade.
    /// * `new_wasm_hash` - 32-byte hash of the newly uploaded WASM binary.
    ///
    /// # Panics
    /// * Panics with [`ContractError::Unauthorized`] if caller is not admin.
    ///
    /// # Side Effects
    /// * Updates contract executable WASM code and emits `ContractUpgraded` event.
    pub fn upgrade_contract(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        if storage::require_admin(&env, &admin).is_err() {
            panic_with_error!(&env, ContractError::Unauthorized);
        }
        env.deployer().update_current_contract_wasm(new_wasm_hash.clone());
        events::emit_contract_upgraded(&env, new_wasm_hash);
    }

    /// Configures the multi-sig signers and threshold required for contract upgrades.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address configuring multi-sig.
    /// * `signers` - Vector of distinct, strictly sorted signer addresses.
    /// * `threshold` - Number of approvals required to execute an upgrade.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    /// * [`ContractError::InvalidSignersList`] - If signers is empty or not strictly sorted.
    /// * [`ContractError::InvalidUpgradeThreshold`] - If threshold is 0 or exceeds signers count.
    ///
    /// # Side Effects
    /// * Stores signers list and threshold in storage and emits `UpgradeSignersConfigured` event.
    pub fn set_upgrade_signers(
        env: Env,
        admin: Address,
        signers: Vec<Address>,
        threshold: u32,
    ) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;

        if signers.is_empty() {
            return Err(ContractError::InvalidSignersList);
        }

        if !is_strictly_sorted_addresses(&signers) {
            return Err(ContractError::InvalidSignersList);
        }

        for i in 0..signers.len() {
            let s = signers.get(i).unwrap();
            validate_address(&s);
        }

        if threshold == 0 || threshold > signers.len() {
            return Err(ContractError::InvalidUpgradeThreshold);
        }

        env.storage()
            .instance()
            .set(&DataKey::UpgradeSigners, &signers);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeThreshold, &threshold);

        events::emit_upgrade_signers_configured(&env, signers, threshold);
        Ok(())
    }

    /// Returns the list of authorized multi-sig upgrade signers.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    pub fn get_upgrade_signers(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::UpgradeSigners)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Returns the threshold number of multi-sig approvals needed to execute an upgrade.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    pub fn get_upgrade_threshold(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::UpgradeThreshold)
            .unwrap_or(0)
    }

    /// Proposes a new WASM hash for multi-sig contract upgrade and automatically casts first approval.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `signer` - Proposing signer address.
    /// * `new_wasm_hash` - 32-byte hash of the candidate WASM binary.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not in upgrade signers list.
    /// * [`ContractError::UpgradeAlreadyProposed`] - If an active upgrade proposal is already pending.
    ///
    /// # Side Effects
    /// * Creates upgrade proposal, records initial approval, and emits `UpgradeProposed` event.
    pub fn propose_upgrade(
        env: Env,
        signer: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        let signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeSigners)
            .unwrap_or_else(|| Vec::new(&env));

        let mut is_valid_signer = false;
        for i in 0..signers.len() {
            if signers.get(i).unwrap() == signer {
                is_valid_signer = true;
                break;
            }
        }

        if !is_valid_signer {
            return Err(ContractError::Unauthorized);
        }

        signer.require_auth();

        if env.storage().instance().has(&DataKey::UpgradeProposal) {
            return Err(ContractError::UpgradeAlreadyProposed);
        }

        let mut approvals = Vec::new(&env);
        approvals.push_back(signer.clone());

        let proposal = crate::types::UpgradeProposal {
            new_wasm_hash: new_wasm_hash.clone(),
            approvals,
            executed: false,
            timestamp: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&DataKey::UpgradeProposal, &proposal);

        events::emit_upgrade_proposed(&env, new_wasm_hash, signer);
        Ok(())
    }

    /// Approves an active pending upgrade proposal on behalf of an authorized signer.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `signer` - Signer address approving the proposal.
    ///
    /// # Errors
    /// * [`ContractError::NoActiveUpgradeProposal`] - If no proposal is pending.
    /// * [`ContractError::Unauthorized`] - If caller is not a configured upgrade signer.
    /// * [`ContractError::AlreadyApproved`] - If signer already approved the proposal.
    ///
    /// # Side Effects
    /// * Appends signer approval to proposal and emits `UpgradeApproved` event.
    pub fn approve_upgrade(env: Env, signer: Address) -> Result<(), ContractError> {
        let signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeSigners)
            .unwrap_or_else(|| Vec::new(&env));

        let mut is_valid_signer = false;
        for i in 0..signers.len() {
            if signers.get(i).unwrap() == signer {
                is_valid_signer = true;
                break;
            }
        }

        if !is_valid_signer {
            return Err(ContractError::Unauthorized);
        }

        signer.require_auth();

        let mut proposal: crate::types::UpgradeProposal = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeProposal)
            .ok_or(ContractError::NoActiveUpgradeProposal)?;

        if proposal.executed {
            return Err(ContractError::NoActiveUpgradeProposal);
        }

        for i in 0..proposal.approvals.len() {
            if proposal.approvals.get(i).unwrap() == signer {
                return Err(ContractError::AlreadyApproved);
            }
        }

        proposal.approvals.push_back(signer.clone());
        let count = proposal.approvals.len();

        env.storage()
            .instance()
            .set(&DataKey::UpgradeProposal, &proposal);

        events::emit_upgrade_approved(&env, proposal.new_wasm_hash, signer, count);
        Ok(())
    }

    /// Executes a pending multi-sig upgrade once approval threshold is met.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    ///
    /// # Errors
    /// * [`ContractError::NoActiveUpgradeProposal`] - If no proposal exists or already executed.
    /// * [`ContractError::InsufficientUpgradeApprovals`] - If approvals count is below threshold.
    ///
    /// # Side Effects
    /// * Updates contract executable WASM code, clears proposal from storage, and emits `ContractUpgraded`.
    pub fn execute_upgrade(env: Env) -> Result<(), ContractError> {
        let threshold: u32 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeThreshold)
            .unwrap_or(0);

        let proposal: crate::types::UpgradeProposal = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeProposal)
            .ok_or(ContractError::NoActiveUpgradeProposal)?;

        if proposal.executed {
            return Err(ContractError::NoActiveUpgradeProposal);
        }

        if proposal.approvals.len() < threshold {
            return Err(ContractError::InsufficientUpgradeApprovals);
        }

        env.deployer()
            .update_current_contract_wasm(proposal.new_wasm_hash.clone());

        env.storage().instance().remove(&DataKey::UpgradeProposal);

        events::emit_contract_upgraded(&env, proposal.new_wasm_hash);
        Ok(())
    }

    /// Cancels an active pending multi-sig upgrade proposal.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address cancelling the proposal.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    /// * [`ContractError::NoActiveUpgradeProposal`] - If no proposal is currently pending.
    ///
    /// # Side Effects
    /// * Removes upgrade proposal from storage and emits `UpgradeProposalCancelled` event.
    pub fn cancel_upgrade(env: Env, admin: Address) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;

        let proposal: crate::types::UpgradeProposal = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeProposal)
            .ok_or(ContractError::NoActiveUpgradeProposal)?;

        if proposal.executed {
            return Err(ContractError::NoActiveUpgradeProposal);
        }

        env.storage().instance().remove(&DataKey::UpgradeProposal);

        events::emit_upgrade_proposal_cancelled(&env, proposal.new_wasm_hash, admin);
        Ok(())
    }

    /// Builds and returns a complete state snapshot of the contract.
    ///
    /// Delegates to [`logic::get_snapshot`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    pub fn get_snapshot(env: Env) -> Result<Snapshot, ContractError> {
        logic::get_snapshot(&env)
    }

    /// Records the current state snapshot to historical storage indexed by timestamp.
    ///
    /// Delegates to [`logic::record_snapshot`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    pub fn record_snapshot(env: Env) -> Result<(), ContractError> {
        logic::record_snapshot(&env)
    }

    /// Retrieves lightweight metadata about current collection sizes.
    ///
    /// Delegates to [`logic::get_snapshot_meta`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    pub fn get_snapshot_meta(env: Env) -> SnapshotMeta {
        logic::get_snapshot_meta(&env)
    }

    /// Retrieves a paginated slice of registered guardians.
    ///
    /// Delegates to [`logic::get_guardians_page`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `offset` - 0-based index offset.
    /// * `limit` - Maximum number of items.
    pub fn get_guardians_page(env: Env, offset: u32, limit: u32) -> Vec<GuardianEntry> {
        logic::get_guardians_page(&env, offset, limit)
    }

    /// Retrieves a paginated slice of registered tasks.
    ///
    /// Delegates to [`logic::get_tasks_page`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `offset` - 0-based index offset.
    /// * `limit` - Maximum number of items.
    pub fn get_tasks_page(env: Env, offset: u32, limit: u32) -> Vec<Task> {
        logic::get_tasks_page(&env, offset, limit)
    }

    /// Retrieves a paginated slice of registered reward streams.
    ///
    /// Delegates to [`logic::get_reward_streams_page`].
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `offset` - 0-based index offset.
    /// * `limit` - Maximum number of items.
    pub fn get_reward_streams_page(env: Env, offset: u32, limit: u32) -> Vec<RewardStream> {
        logic::get_reward_streams_page(&env, offset, limit)
    }

    /// Returns the history of timestamps at which state snapshots were recorded.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    pub fn get_snapshot_history(env: Env) -> soroban_sdk::Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::SnapshotTimestamps)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env))
    }

    /// Retrieves a historical state snapshot by its recording timestamp.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `timestamp` - Exact timestamp when the snapshot was recorded.
    ///
    /// # Errors
    /// * [`ContractError::SnapshotNotFound`] - If no snapshot exists for given timestamp.
    pub fn get_snapshot_at(env: Env, timestamp: u64) -> Result<Snapshot, ContractError> {
        env.storage()
            .instance()
            .get(&DataKey::SnapshotAt(timestamp))
            .ok_or(ContractError::SnapshotNotFound)
    }

    /// Queries the withdrawal timelock expiration timestamp for a guardian.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `guardian` - Address of guardian.
    ///
    /// # Returns
    /// * `Some(u64)` with unlock timestamp, or `None` if no timelock is active.
    pub fn get_withdrawal_timelock(env: Env, guardian: Address) -> Option<u64> {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawalTimelock(guardian))
    }

    /// Executes multiple contract calls atomically in a single transaction.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `calls` - Vector of [`BatchCall`] enum items to execute.
    ///
    /// # Errors
    /// * Any error returned by individual call components will abort and revert batch.
    pub fn batch_execute(
        env: Env,
        calls: soroban_sdk::Vec<BatchCall>,
    ) -> Result<(), ContractError> {
        for i in 0..calls.len() {
            let call = calls.get(i).unwrap();
            match call {
                BatchCall::Vote { guardian, task_id } => {
                    logic::process_vote(&env, guardian, task_id)?;
                }
                BatchCall::RegisterTask {
                    admin,
                    task_id,
                    min_votes_required,
                } => {
                    storage::require_admin(&env, &admin)?;
                    task::register_task(&env, task_id, min_votes_required)?;
                    events::emit_task_registered(&env, task_id, min_votes_required);
                }
                BatchCall::SetReputation {
                    admin,
                    guardian,
                    score,
                } => {
                    storage::require_admin(&env, &admin)?;
                    reputation::set_reputation(&env, &guardian, score)?;
                    events::emit_reputation_updated(&env, guardian, score);
                }
                BatchCall::CancelTask { admin, task_id } => {
                    storage::require_admin(&env, &admin)?;
                    task::cancel_task(&env, task_id)?;
                    events::emit_task_cancelled(&env, task_id);
                }
                BatchCall::RecordFailure { reporter } => {
                    circuit_breaker::record_failure(&env, reporter)?;
                }
                BatchCall::ArchiveTask { admin, task_id } => {
                    storage::require_admin(&env, &admin)?;
                    task::archive_task(&env, task_id)?;
                    events::emit_task_archived(&env, task_id);
                }
                BatchCall::PurgeTask { admin, task_id } => {
                    storage::require_admin(&env, &admin)?;
                    task::purge_task(&env, task_id)?;
                    events::emit_task_purged(&env, task_id);
                }
                BatchCall::StartRewardStream {
                    admin,
                    drips_address,
                    contributor,
                    task_id,
                } => {
                    storage::require_admin(&env, &admin)?;
                    validate_address(&drips_address);
                    validate_address(&contributor);
                    drips::start_reward_stream(&env, drips_address, contributor, task_id)?;
                }
            }
        }
        Ok(())
    }

    /// Grants a specific role to a target address.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `caller` - Address initiating role grant (must hold Admin role).
    /// * `target` - Recipient address receiving role.
    /// * `role` - Role variant to grant.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller does not hold Admin role.
    ///
    /// # Side Effects
    /// * Grants role in RBAC storage and emits `RoleGranted` event.
    pub fn grant_role(
        env: Env,
        caller: Address,
        target: Address,
        role: crate::types::Role,
    ) -> Result<(), ContractError> {
        crate::contracts::rbac::grant_role(&env, &caller, &target, role)
    }

    /// Revokes a specific role from a target address.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `caller` - Address initiating revocation (must hold Admin role).
    /// * `target` - Address losing role.
    /// * `role` - Role variant to revoke.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller does not hold Admin role.
    ///
    /// # Side Effects
    /// * Clears role in RBAC storage and emits `RoleRevoked` event.
    pub fn revoke_role(
        env: Env,
        caller: Address,
        target: Address,
        role: crate::types::Role,
    ) -> Result<(), ContractError> {
        crate::contracts::rbac::revoke_role(&env, &caller, &target, role)
    }

    /// Checks if an address holds a specific role.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `address` - Address being queried.
    /// * `role` - Role variant being verified.
    pub fn has_role(env: Env, address: Address, role: crate::types::Role) -> bool {
        crate::contracts::rbac::has_role(&env, &address, role)
    }

    /// Queries the current schema version of contract storage.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    pub fn get_storage_version(env: Env) -> u32 {
        storage::get_storage_version(&env)
    }

    /// Migrates contract storage schema to the latest version.
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `admin` - Admin address authorizing the migration.
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - If caller is not admin.
    ///
    /// # Side Effects
    /// * Executes migration logic and updates stored schema version.
    pub fn migrate_storage(env: Env, admin: Address) -> Result<(), ContractError> {
        storage::require_admin(&env, &admin)?;
        crate::migrate::migrate_storage(&env)
    }
}
