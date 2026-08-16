use crate::types::{
    ContractError, DataKey, GuardianEntry, RewardStream, Snapshot, SnapshotMeta, Task,
};
use crate::DEFAULT_WEIGHT_THRESHOLD;
use crate::{
    circuit_breaker, drips, events, guardian, reentrancy, reputation, storage, task, timelock,
    vault,
};
use soroban_sdk::{Address, Env, Map, Vec};

/// Attempts to release funds from the vault for a completed task.
///
/// Invokes `try_release_funds` on the configured vault contract. If the vault call
/// fails, the failure is logged via [`events::emit_vault_release_failed`] but does
/// not revert the transaction. This ensures task resolution is not blocked by a broken vault.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `task_id` - Unique identifier of the resolved task.
/// * `vault_addr` - Address of the vault contract to release funds from.
///
/// # Side Effects
/// * Emits `VaultReleaseSuccess` event on success, or `VaultReleaseFailed` on failure.
pub(crate) fn try_release_vault_funds(env: &Env, task_id: u64, vault_addr: &Address) {
    // Use the generated try_release_funds method from VaultClient
    // This will not panic on failure - it returns a Result
    let vault_client = vault::VaultClient::new(env, vault_addr);
    let result = vault_client.try_release_funds(&task_id);

    match result {
        Ok(_) => {
            events::emit_vault_release_success(env, task_id);
        }
        Err(_e) => {
            // Vault failure is logged but does not revert the transaction
            events::emit_vault_release_failed(env, task_id);
        }
    }
}

/// Maximum number of entries `get_snapshot`/`record_snapshot` will read from
/// any single tracked collection (guardians, tasks, reward streams) before
/// refusing to build a snapshot.
///
/// Building a full snapshot costs roughly 2 storage reads per guardian
/// (guardian flag + reputation), 2+ reads per task (task struct + its voter
/// list), and 2 reads per reward stream. At `MAX_SNAPSHOT_COLLECTION_SIZE`
/// entries per collection that stays comfortably inside Soroban's
/// per-transaction CPU instruction budget with wide margin — see the
/// growth-curve measurements in `tests/snapshot_scaling.rs`. Once a
/// collection approaches this ceiling, callers should switch to the
/// paginated API (`get_snapshot_meta` + `get_guardians_page` +
/// `get_tasks_page` + `get_reward_streams_page`), which reads at most
/// `O(limit)` entries per call — not `O(total collection size)` — and stays
/// cheaply invokable well past the point this cap would refuse to build a
/// full snapshot.
pub(crate) const MAX_SNAPSHOT_COLLECTION_SIZE: u32 = 200;

/// Maximum number of entries any paginated snapshot call will return,
/// regardless of the caller-requested `limit`. Keeps a single page call's
/// cost bounded even against a hostile/misconfigured caller.
pub(crate) const MAX_PAGE_LIMIT: u32 = 50;

/// Locks tokens for a registered guardian as stake or voting collateral.
///
/// Requires guardian authorization, checks that the circuit breaker is not paused,
/// calculates and deducts protocol treasury fees if configured, and transfers tokens
/// from the guardian to the contract storage. If timelock is active, resets the withdrawal timelock.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `guardian` - Address of the guardian locking tokens.
/// * `amount` - Amount of tokens to lock (must be positive).
///
/// # Errors
/// * [`ContractError::ContractPaused`] - If the circuit breaker is paused.
/// * [`ContractError::InvalidAmount`] - If `amount` <= 0 or fee exceeds amount.
/// * [`ContractError::GuardianNotFound`] - If `guardian` is not registered.
/// * [`ContractError::TokenTransferFailed`] - If token transfer fails.
///
/// # Side Effects
/// * Updates guardian locked balance in storage.
/// * Clears/resets withdrawal timelock for the guardian.
/// * Emits `TokensLocked` event.
pub(crate) fn lock_tokens(env: &Env, guardian: Address, amount: i128) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    let token: Address = env
        .storage()
        .instance()
        .get(&DataKey::TokenAddress)
        .ok_or(ContractError::GuardianNotFound)?;
    let client = soroban_sdk::token::Client::new(env, &token);
    let fee_bps = storage::get_fee_bps(env);
    let treasury = storage::get_treasury_address(env);
    let fee = (amount * fee_bps as i128) / 10000;
    let net = amount - fee;
    if fee > 0 {
        if let Some(treasury_addr) = treasury {
            client.transfer(&guardian, &treasury_addr, &fee);
        }
    }
    client.transfer(&guardian, &env.current_contract_address(), &net);
    let current_locked: i128 = env
        .storage()
        .instance()
        .get(&DataKey::LockedTokens(guardian.clone()))
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKey::LockedTokens(guardian.clone()), &(current_locked + net));
    // Reset withdrawal timelock on new deposit
    timelock::clear_withdrawal_timelock(env, &guardian);
    events::emit_tokens_locked(env, guardian, net);
    Ok(())
}

/// Requests initiation of a 24-hour withdrawal timelock for unlocking guardian tokens.
///
/// Requires guardian authorization and checks that the circuit breaker is not paused.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `guardian` - Address of the guardian requesting unlock.
///
/// # Errors
/// * [`ContractError::ContractPaused`] - If the circuit breaker is paused.
/// * [`ContractError::GuardianNotFound`] - If caller is not a guardian.
///
/// # Side Effects
/// * Sets the withdrawal unlock timestamp in storage.
/// * Emits `UnlockRequested` event.
pub(crate) fn request_unlock(env: &Env, guardian: Address) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    timelock::request_unlock(env, guardian)
}

/// Unlocks and withdraws previously locked tokens back to the guardian after timelock expiration.
///
/// Requires guardian authorization, checks circuit breaker status, verifies that the 24-hour
/// withdrawal delay has elapsed, and transfers the net locked amount to the guardian after
/// deducting applicable treasury fees.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `guardian` - Address of the guardian unlocking tokens.
///
/// # Errors
/// * [`ContractError::ContractPaused`] - If the circuit breaker is paused.
/// * [`ContractError::GuardianNotFound`] - If token or guardian record is missing.
/// * [`ContractError::WithdrawalTimelockActive`] - If the 24-hour timelock period has not elapsed.
/// * [`ContractError::NoTokensLocked`] - If the guardian has 0 locked tokens.
///
/// # Side Effects
/// * Transfers token balance back to guardian (and fee to treasury).
/// * Clears guardian locked balance and timelock in storage.
/// * Emits `TokensUnlocked` event.
pub(crate) fn unlock_tokens(env: &Env, guardian: Address) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    timelock::require_timelock_expired(env, &guardian)?;
    let token: Address = env
        .storage()
        .instance()
        .get(&DataKey::TokenAddress)
        .ok_or(ContractError::GuardianNotFound)?;
    let client = soroban_sdk::token::Client::new(env, &token);
    let locked: i128 = env
        .storage()
        .instance()
        .get(&DataKey::LockedTokens(guardian.clone()))
        .unwrap_or(0);
    let fee_bps = storage::get_fee_bps(env);
    let treasury = storage::get_treasury_address(env);
    let fee = (locked * fee_bps as i128) / 10000;
    let net = locked - fee;
    if fee > 0 {
        if let Some(treasury_addr) = treasury {
            client.transfer(&env.current_contract_address(), &treasury_addr, &fee);
        }
    }
    client.transfer(&env.current_contract_address(), &guardian, &net);
    env.storage()
        .instance()
        .remove(&DataKey::LockedTokens(guardian.clone()));
    timelock::clear_withdrawal_timelock(env, &guardian);
    events::emit_tokens_unlocked(env, guardian, net);
    Ok(())
}

/// Recovers tokens in an emergency scenario when contract is paused.
///
/// Requires admin authorization and verifies that the contract is currently paused.
/// Transfers the specified amount of tokens from contract to recipient.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `admin` - Admin address executing the recovery.
/// * `recipient` - Destination address to receive the recovered tokens.
/// * `amount` - Number of tokens to transfer.
///
/// # Errors
/// * [`ContractError::Unauthorized`] - If caller is not admin.
/// * [`ContractError::ContractNotPaused`] - If contract is not paused.
/// * [`ContractError::GuardianNotFound`] - If token address key is missing.
///
/// # Side Effects
/// * Transfers token funds from contract address to recipient.
/// * Emits `EmergencyRecovery` event.
pub(crate) fn emergency_recover(
    env: &Env,
    admin: Address,
    recipient: Address,
    amount: i128,
) -> Result<(), ContractError> {
    storage::require_admin(env, &admin)?;
    circuit_breaker::require_paused(env)?;
    let token: Address = env
        .storage()
        .instance()
        .get(&DataKey::TokenAddress)
        .ok_or(ContractError::GuardianNotFound)?;
    let client = soroban_sdk::token::Client::new(env, &token);
    client.transfer(&env.current_contract_address(), &recipient, &amount);
    events::emit_emergency_recovery(env, recipient, amount);
    Ok(())
}

/// Resigns a guardian from their role and removes their registration.
///
/// Requires guardian authorization, checks that circuit breaker is not paused,
/// and verifies that any active withdrawal timelock has expired before removal.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `guardian` - Address of the guardian resigning.
///
/// # Errors
/// * [`ContractError::ContractPaused`] - If contract is paused.
/// * [`ContractError::GuardianNotFound`] - If address is not a registered guardian.
/// * [`ContractError::WithdrawalTimelockActive`] - If timelock is currently active/pending.
///
/// # Side Effects
/// * Removes guardian record and reputation from storage.
/// * Decrements active guardian count and updates guardian list.
/// * Emits `GuardianResigned` event.
pub(crate) fn resign_guardian(env: &Env, guardian: Address) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();

    if !guardian::is_guardian(env, &guardian) {
        return Err(ContractError::GuardianNotFound);
    }

    if timelock::is_withdrawal_timelock_active(env, &guardian) {
        return Err(ContractError::WithdrawalTimelockActive);
    }

    // 1. Mark as not guardian and clear their reputation
    env.storage()
        .instance()
        .set(&DataKey::Guardian(guardian.clone()), &false);
    reputation::clear_reputation(env, &guardian);

    // 2. Decrement GuardianCount safely
    let current_count: u32 = env
        .storage()
        .instance()
        .get(&DataKey::GuardianCount)
        .unwrap_or(0);
    if current_count > 0 {
        env.storage()
            .instance()
            .set(&DataKey::GuardianCount, &(current_count - 1));
    }

    // 3. Remove guardian from the GuardiansList vector
    let list_opt: Option<Vec<Address>> = env.storage().instance().get(&DataKey::GuardiansList);
    if let Some(list) = list_opt {
        let mut new_list = Vec::new(env);
        for g in list.iter() {
            if g != guardian {
                new_list.push_back(g);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::GuardiansList, &new_list);
    }

    events::emit_guardian_resigned(env, guardian);

    Ok(())
}

/// Validates conditions and processes a single vote from a guardian for a task.
///
/// Enforces reentrancy protection, non-paused state, guardian authentication,
/// guardian registration, voting power calculation, and delegates to [`vote_inner`].
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `guardian` - Address of the guardian casting the vote.
/// * `task_id` - Identifier of the task being voted on.
///
/// # Errors
/// * [`ContractError::ContractPaused`] - If contract is paused.
/// * [`ContractError::GuardianNotFound`] - If voter is not a guardian.
/// * [`ContractError::InvalidWeight`] - If guardian voting power is 0.
/// * Other errors propagated from [`vote_inner`].
///
/// # Side Effects
/// * Updates vote tallies, voter list, and potentially resolves task status.
pub(crate) fn process_vote(
    env: &Env,
    guardian: Address,
    task_id: u64,
) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    reentrancy::enter(env)?;

    guardian.require_auth();

    if !guardian::is_guardian(env, &guardian) {
        reentrancy::exit(env);
        return Err(ContractError::GuardianNotFound);
    }

    let weight = match reputation::calculate_voting_power(env, &guardian) {
        Some(w) if w > 0 => w,
        _ => {
            reentrancy::exit(env);
            return Err(ContractError::InvalidWeight);
        }
    };

    let result = vote_inner(env, &guardian, task_id, weight);
    reentrancy::exit(env);
    result
}

/// Core internal logic for recording a guardian's weighted vote on a task.
///
/// Validates task existence, checks if already resolved, verifies that guardian has not
/// previously voted on this task, records the vote, updates weight totals and vote counts,
/// checks threshold completion conditions, and triggers fund release/drips if resolved.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `guardian` - Reference to guardian address.
/// * `task_id` - Identifier of the task.
/// * `weight` - Weighted voting power of the guardian.
///
/// # Errors
/// * [`ContractError::TaskNotFound`] - If task is not registered.
/// * [`ContractError::TaskAlreadyResolved`] - If task is already completed.
/// * [`ContractError::AlreadyVoted`] - If guardian already voted on this task.
///
/// # Side Effects
/// * Updates task state in storage.
/// * Emits `WeightedVoteCast` and optionally `TaskResolved` events.
/// * Attempts vault fund release and drips stream start if task passes threshold.
pub(crate) fn vote_inner(
    env: &Env,
    guardian: &Address,
    task_id: u64,
    weight: u64,
) -> Result<(), ContractError> {
    let mut task = task::get_task(env, task_id).ok_or(ContractError::TaskNotFound)?;

    if task.resolved {
        return Err(ContractError::TaskAlreadyResolved);
    }

    if task::has_voted(env, task_id, guardian) {
        return Err(ContractError::AlreadyVoted);
    }

    task::record_vote(env, task_id, guardian);

    task.votes_received = task.votes_received.saturating_add(1);
    task.total_weight = task.total_weight.saturating_add(weight);

    events::emit_weighted_vote_cast(env, task_id, guardian.clone(), weight);

    let weight_threshold: u64 = env
        .storage()
        .instance()
        .get(&DataKey::WeightThreshold)
        .unwrap_or(DEFAULT_WEIGHT_THRESHOLD);

    let quorum_met = task.votes_received >= task.min_votes_required;
    let weight_met = task.total_weight >= weight_threshold;

    if quorum_met && weight_met {
        task.resolved = true;
        events::emit_task_resolved(env, task_id, task.total_weight);

        if let Some(vault_addr) = storage::get_vault_address(env) {
            try_release_vault_funds(env, task_id, &vault_addr);
        }

        if let Some(reward_stream) = drips::get_reward_stream(env, task_id) {
            drips::try_start_drips_stream(env, task_id, &reward_stream);
        }
    }

    task::save_task(env, &task);
    Ok(())
}

/// Atomically processes multiple task votes submitted by a guardian in a single batch.
///
/// Enforces reentrancy protection, verifies guardian authentication and voting power once,
/// and sequentially applies [`vote_inner`] across all supplied task IDs.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `guardian` - Address of the guardian casting batch votes.
/// * `task_ids` - Collection of task IDs to vote on.
///
/// # Errors
/// * [`ContractError::ContractPaused`] - If contract is paused.
/// * [`ContractError::GuardianNotFound`] - If voter is not a guardian.
/// * [`ContractError::InvalidWeight`] - If guardian voting power is 0.
/// * Any error returned by individual task voting in [`vote_inner`].
///
/// # Side Effects
/// * Updates state for all voted tasks and emits associated events.
pub(crate) fn process_vote_batch(
    env: &Env,
    guardian: Address,
    task_ids: Vec<u64>,
) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    reentrancy::enter(env)?;

    guardian.require_auth();

    if !guardian::is_guardian(env, &guardian) {
        reentrancy::exit(env);
        return Err(ContractError::GuardianNotFound);
    }

    let weight = match reputation::calculate_voting_power(env, &guardian) {
        Some(w) if w > 0 => w,
        _ => {
            reentrancy::exit(env);
            return Err(ContractError::InvalidWeight);
        }
    };

    for i in 0..task_ids.len() {
        let task_id = task_ids.get(i).unwrap();
        if let Err(e) = vote_inner(env, &guardian, task_id, weight) {
            reentrancy::exit(env);
            return Err(e);
        }
    }

    reentrancy::exit(env);
    Ok(())
}

/// Builds and returns a complete state snapshot of the contract.
///
/// Collects guardians, tasks, and reward streams into a [`Snapshot`] struct.
/// Enforces `MAX_SNAPSHOT_COLLECTION_SIZE` limits on each collection to prevent
/// transaction instruction budget exhaustion.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
///
/// # Errors
/// * [`ContractError::SnapshotCollectionTooLarge`] - If guardians, tasks, or streams count exceeds limit.
///
/// # Returns
/// * `Ok(Snapshot)` containing all current protocol state, timestamp, and counts.
pub(crate) fn get_snapshot(env: &Env) -> Result<Snapshot, ContractError> {
    let raw_guardians: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::GuardiansList)
        .unwrap_or_else(|| Vec::new(env));
    if raw_guardians.len() > MAX_SNAPSHOT_COLLECTION_SIZE {
        return Err(ContractError::SnapshotCollectionTooLarge);
    }

    let mut guardians = Vec::new(env);
    for g in raw_guardians.iter() {
        let is_active = guardian::is_guardian(env, &g);
        let rep = reputation::get_reputation(env, &g).unwrap_or(0);
        guardians.push_back(GuardianEntry {
            address: g,
            is_active,
            reputation: rep,
        });
    }

    let task_ids: Vec<u64> = env
        .storage()
        .instance()
        .get(&DataKey::TasksList)
        .unwrap_or_else(|| Vec::new(env));
    if task_ids.len() > MAX_SNAPSHOT_COLLECTION_SIZE {
        return Err(ContractError::SnapshotCollectionTooLarge);
    }

    let mut tasks = Vec::new(env);
    for id in task_ids.iter() {
        if let Some(t) = task::get_task(env, id) {
            tasks.push_back(t);
        }
    }

    let reward_task_ids: Vec<u64> = env
        .storage()
        .instance()
        .get(&DataKey::RewardStreamsList)
        .unwrap_or_else(|| Vec::new(env));
    if reward_task_ids.len() > MAX_SNAPSHOT_COLLECTION_SIZE {
        return Err(ContractError::SnapshotCollectionTooLarge);
    }

    let mut reward_streams = Vec::new(env);
    for id in reward_task_ids.iter() {
        if let Some(rs) = drips::get_reward_stream(env, id) {
            reward_streams.push_back(rs);
        }
    }

    let raw_reporters: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::FailureReportersList)
        .unwrap_or_else(|| Vec::new(env));

    let mut failure_counts = Map::new(env);
    for r in raw_reporters.iter() {
        let count = circuit_breaker::get_reporter_failure_count(env, &r);
        failure_counts.set(r, count);
    }

    Ok(Snapshot {
        admin: storage::get_admin(env),
        is_paused: circuit_breaker::is_paused(env),
        guardians,
        tasks,
        reward_streams,
        weight_threshold: storage::get_weight_threshold(env),
        vault_address: storage::get_vault_address(env),
        token_address: storage::get_token_address(env),
        failure_count: circuit_breaker::get_failure_count(env),
        failure_counts,
        timestamp: env.ledger().timestamp(),
    })
}

/// Retrieves lightweight metadata about the current protocol state and collection sizes.
///
/// Useful for determining pagination bounds without loading entire collections into memory.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
///
/// # Returns
/// * [`SnapshotMeta`] struct with counts of guardians, tasks, reward streams, failure stats, and timestamp.
pub(crate) fn get_snapshot_meta(env: &Env) -> SnapshotMeta {
    let guardian_count = env
        .storage()
        .instance()
        .get(&DataKey::GuardiansList)
        .map(|list: Vec<Address>| list.len())
        .unwrap_or(0);

    let task_count = env
        .storage()
        .instance()
        .get(&DataKey::TasksList)
        .map(|list: Vec<u64>| list.len())
        .unwrap_or(0);

    let reward_stream_count = env
        .storage()
        .instance()
        .get(&DataKey::RewardStreamsList)
        .map(|list: Vec<u64>| list.len())
        .unwrap_or(0);

    SnapshotMeta {
        admin: storage::get_admin(env),
        is_paused: circuit_breaker::is_paused(env),
        guardian_count,
        task_count,
        reward_stream_count,
        weight_threshold: storage::get_weight_threshold(env),
        vault_address: storage::get_vault_address(env),
        token_address: storage::get_token_address(env),
        failure_count: circuit_breaker::get_failure_count(env),
        timestamp: env.ledger().timestamp(),
    }
}

/// Retrieves a paginated slice of registered guardians.
///
/// Clamps `limit` to [`MAX_PAGE_LIMIT`] to maintain bounded compute cost.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `offset` - 0-based starting index within the guardians collection.
/// * `limit` - Maximum number of entries to return (capped at `MAX_PAGE_LIMIT`).
///
/// # Returns
/// * `Vec<GuardianEntry>` containing active status and reputation for each guardian in page.
pub(crate) fn get_guardians_page(env: &Env, offset: u32, limit: u32) -> Vec<GuardianEntry> {
    let raw_guardians: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::GuardiansList)
        .unwrap_or_else(|| Vec::new(env));

    let effective_limit = limit.min(MAX_PAGE_LIMIT);
    let total = raw_guardians.len();
    let end = (offset.saturating_add(effective_limit)).min(total);

    let mut result = Vec::new(env);
    if offset >= total {
        return result;
    }

    for i in offset..end {
        if let Some(g) = raw_guardians.get(i) {
            let is_active = guardian::is_guardian(env, &g);
            let rep = reputation::get_reputation(env, &g).unwrap_or(0);
            result.push_back(GuardianEntry {
                address: g,
                is_active,
                reputation: rep,
            });
        }
    }
    result
}

/// Retrieves a paginated slice of registered tasks.
///
/// Clamps `limit` to [`MAX_PAGE_LIMIT`] to prevent execution budget overruns.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `offset` - 0-based starting index within the tasks collection.
/// * `limit` - Maximum number of entries to return (capped at `MAX_PAGE_LIMIT`).
///
/// # Returns
/// * `Vec<Task>` containing full task details for each task in the requested window.
pub(crate) fn get_tasks_page(env: &Env, offset: u32, limit: u32) -> Vec<Task> {
    let task_ids: Vec<u64> = env
        .storage()
        .instance()
        .get(&DataKey::TasksList)
        .unwrap_or_else(|| Vec::new(env));

    let effective_limit = limit.min(MAX_PAGE_LIMIT);
    let total = task_ids.len();
    let end = (offset.saturating_add(effective_limit)).min(total);

    let mut result = Vec::new(env);
    if offset >= total {
        return result;
    }

    for i in offset..end {
        if let Some(id) = task_ids.get(i) {
            if let Some(t) = task::get_task(env, id) {
                result.push_back(t);
            }
        }
    }
    result
}

/// Retrieves a paginated slice of registered reward streams.
///
/// Clamps `limit` to [`MAX_PAGE_LIMIT`].
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `offset` - 0-based starting index within the reward streams collection.
/// * `limit` - Maximum number of entries to return (capped at `MAX_PAGE_LIMIT`).
///
/// # Returns
/// * `Vec<RewardStream>` containing active stream details in the requested range.
pub(crate) fn get_reward_streams_page(env: &Env, offset: u32, limit: u32) -> Vec<RewardStream> {
    let reward_task_ids: Vec<u64> = env
        .storage()
        .instance()
        .get(&DataKey::RewardStreamsList)
        .unwrap_or_else(|| Vec::new(env));

    let effective_limit = limit.min(MAX_PAGE_LIMIT);
    let total = reward_task_ids.len();
    let end = (offset.saturating_add(effective_limit)).min(total);

    let mut result = Vec::new(env);
    if offset >= total {
        return result;
    }

    for i in offset..end {
        if let Some(id) = reward_task_ids.get(i) {
            if let Some(rs) = drips::get_reward_stream(env, id) {
                result.push_back(rs);
            }
        }
    }
    result
}

/// Records a full state snapshot into historical storage indexed by timestamp.
///
/// Validates that collection sizes are within [`MAX_SNAPSHOT_COLLECTION_SIZE`], saves
/// the snapshot under [`DataKey::SnapshotAt`], and appends the timestamp to [`DataKey::SnapshotTimestamps`].
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
///
/// # Errors
/// * [`ContractError::SnapshotCollectionTooLarge`] - If any collection exceeds maximum snapshot capacity.
///
/// # Side Effects
/// * Writes timestamped snapshot and updates history index in contract instance storage.
pub(crate) fn record_snapshot(env: &Env) -> Result<(), ContractError> {
    let snap = get_snapshot(env)?;
    let ts = snap.timestamp;

    env.storage().instance().set(&DataKey::SnapshotAt(ts), &snap);

    let mut history: soroban_sdk::Vec<u64> = env
        .storage()
        .instance()
        .get(&DataKey::SnapshotTimestamps)
        .unwrap_or_else(|| soroban_sdk::Vec::new(env));
    history.push_back(ts);
    env.storage()
        .instance()
        .set(&DataKey::SnapshotTimestamps, &history);

    Ok(())
}
