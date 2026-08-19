use crate::types::{
    ContractError, DataKey, GuardianEntry, RewardStream, Snapshot, SnapshotMeta, Task,
};
use crate::DEFAULT_WEIGHT_THRESHOLD;
use crate::{
    circuit_breaker, drips, events, guardian, limits, reentrancy, reputation, storage, task,
    timelock, vault,
};
use soroban_sdk::{Address, Env, Map, Vec};

/// Attempts to release funds from the vault. If the vault call fails, the failure
/// is logged via an event but does not revert the transaction. This ensures task
/// resolution is not blocked by a broken vault.
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

pub(crate) fn lock_tokens(env: &Env, guardian: Address, amount: i128) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    let token: Address = env
        .storage()
        .instance()
        .get(&DataKey::TokenAddress)
        .ok_or(ContractError::NotInitialized)?;

    let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
    let mut net_amount = amount;

    if fee_bps > 0 {
        let fee_amount = (amount * (fee_bps as i128)) / 10000;
        if fee_amount > 0 {
            if let Some(treasury) = env
                .storage()
                .instance()
                .get::<_, Address>(&DataKey::TreasuryAddress)
            {
                let token_client = soroban_sdk::token::Client::new(env, &token);
                token_client.transfer(&guardian, &treasury, &fee_amount);
                net_amount = amount - fee_amount;
            }
        }
    }

    let token_client = soroban_sdk::token::Client::new(env, &token);
    token_client.transfer(&guardian, &env.current_contract_address(), &net_amount);
    let key = DataKey::LockedBalance(guardian.clone());
    let prev: i128 = env.storage().instance().get(&key).unwrap_or(0);
    env.storage().instance().set(&key, &(prev + net_amount));
    events::emit_tokens_locked(env, &guardian, amount);
    Ok(())
}

pub(crate) fn request_unlock(env: &Env, guardian: Address) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    timelock::initiate_withdrawal(env, guardian.clone());
    events::emit_timelock_started(env, &guardian);
    Ok(())
}

pub(crate) fn unlock_tokens(env: &Env, guardian: Address) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    if guardian::is_guardian(env, &guardian) {
        return Err(ContractError::StillGuardian);
    }

    // Check if timelock has expired
    timelock::check_timelock_expired(env, &guardian)?;

    let key = DataKey::LockedBalance(guardian.clone());
    let amount: i128 = env.storage().instance().get(&key).unwrap_or(0);
    if amount > 0 {
        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenAddress)
            .ok_or(ContractError::NotInitialized)?;
        let token_client = soroban_sdk::token::Client::new(env, &token);

        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
        let mut net_amount = amount;

        if fee_bps > 0 {
            let fee_amount = (amount * (fee_bps as i128)) / 10000;
            if fee_amount > 0 {
                if let Some(treasury) = env
                    .storage()
                    .instance()
                    .get::<_, Address>(&DataKey::TreasuryAddress)
                {
                    token_client.transfer(&env.current_contract_address(), &treasury, &fee_amount);
                    net_amount = amount - fee_amount;
                }
            }
        }

        token_client.transfer(&env.current_contract_address(), &guardian, &net_amount);
        env.storage().instance().set(&key, &0i128);
    }

    // Clear the timelock after successful withdrawal
    timelock::clear_timelock(env, &guardian);
    events::emit_tokens_unlocked(env, &guardian, amount);
    Ok(())
}

pub(crate) fn emergency_recover(
    env: &Env,
    admin: Address,
    recipient: Address,
    amount: i128,
) -> Result<(), ContractError> {
    crate::validation::validate_external_address(env, &recipient)?;
    crate::validation::validate_token_amount(amount)?;

    let token: Address = env
        .storage()
        .instance()
        .get(&DataKey::TokenAddress)
        .ok_or(ContractError::NotInitialized)?;
    let contract_address = env.current_contract_address();
    let token_client = soroban_sdk::token::Client::new(env, &token);
    token_client.transfer(&contract_address, &recipient, &amount);
    events::emit_emergency_recovery(env, &admin, &recipient, amount);
    Ok(())
}

pub(crate) fn resign_guardian(env: &Env, guardian: Address) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    if !guardian::is_guardian(env, &guardian) {
        return Err(ContractError::NotGuardian);
    }

    // Check if timelock has expired
    timelock::check_timelock_expired(env, &guardian)?;

    let g_key = DataKey::Guardian(guardian.clone());
    env.storage().instance().remove(&g_key);
    let key = DataKey::LockedBalance(guardian.clone());
    let amount: i128 = env.storage().instance().get(&key).unwrap_or(0);
    if amount > 0 {
        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenAddress)
            .ok_or(ContractError::NotInitialized)?;
        let token_client = soroban_sdk::token::Client::new(env, &token);

        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
        let mut net_amount = amount;

        if fee_bps > 0 {
            let fee_amount = (amount * (fee_bps as i128)) / 10000;
            if fee_amount > 0 {
                if let Some(treasury) = env
                    .storage()
                    .instance()
                    .get::<_, Address>(&DataKey::TreasuryAddress)
                {
                    token_client.transfer(&env.current_contract_address(), &treasury, &fee_amount);
                    net_amount = amount - fee_amount;
                }
            }
        }

        token_client.transfer(&env.current_contract_address(), &guardian, &net_amount);
        env.storage().instance().set(&key, &0i128);
    }

    // Clear the timelock after successful resignation
    timelock::clear_timelock(env, &guardian);
    events::emit_guardian_resigned(env, &guardian);
    Ok(())
}

pub(crate) fn process_vote(
    env: &Env,
    guardian: Address,
    task_id: u64,
) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    reentrancy::lock(env)?;

    if !guardian::is_guardian(env, &guardian) {
        reentrancy::unlock(env);
        return Err(ContractError::NotAuthorized);
    }

    let token_key = DataKey::TokenAddress;
    if !env.storage().instance().has(&token_key) {
        reentrancy::unlock(env);
        return Err(ContractError::NotInitialized);
    }
    let threshold: i128 = env
        .storage()
        .instance()
        .get(&DataKey::LockThreshold)
        .unwrap_or(0);
    let balance_key = DataKey::LockedBalance(guardian.clone());
    let locked_balance: i128 = env.storage().instance().get(&balance_key).unwrap_or(0);

    if locked_balance <= threshold {
        reentrancy::unlock(env);
        return Err(ContractError::InsufficientLockedBalance);
    }

    let voted_key = DataKey::Voted(task_id, guardian.clone());
    if env.storage().instance().has(&voted_key) {
        reentrancy::unlock(env);
        return Err(ContractError::DuplicateVote);
    }

    let weight = match reputation::get_rep(env, &guardian) {
        Ok(w) => w,
        Err(e) => {
            reentrancy::unlock(env);
            return Err(e);
        }
    };

    if weight == 0 {
        reentrancy::unlock(env);
        return Err(ContractError::ZeroWeightVote);
    }

    let mut t = match storage::get_active_task(env, task_id) {
        Some(t) => t,
        None => {
            reentrancy::unlock(env);
            return Err(ContractError::TaskNotFound);
        }
    };

    if t.is_cancelled {
        reentrancy::unlock(env);
        return Err(ContractError::TaskCancelled);
    }

    let weight_threshold: u64 = env
        .storage()
        .instance()
        .get(&DataKey::WeightThreshold)
        .unwrap_or(DEFAULT_WEIGHT_THRESHOLD);

    // Keep this transition delegated to the Kani-verified consensus module.
    // Reimplementing it here could make the on-chain path diverge from the
    // arithmetic proved in `verification/`; see
    // [VERIFICATION_REPORT.md](../../../docs/history/VERIFICATION_REPORT.md).
    let mut consensus_state = crate::consensus::ConsensusState {
        total_weight_accrued: t.total_weight_accrued,
        votes: t.votes,
        is_done: t.is_done,
    };
    if let Err(e) = crate::consensus::apply_vote(&mut consensus_state, weight, weight_threshold) {
        reentrancy::unlock(env);
        return Err(match e {
            crate::consensus::ConsensusError::WeightOverflow => ContractError::WeightOverflow,
            crate::consensus::ConsensusError::ZeroWeight => ContractError::ZeroWeightVote,
        });
    }
    t.total_weight_accrued = consensus_state.total_weight_accrued;
    t.votes = consensus_state.votes;

    if consensus_state.is_done && t.votes >= t.min_votes_required && !t.is_done {
        t.is_done = true;
        t.resolved_at = env.ledger().timestamp();
        events::emit_task_resolved(env, task_id, t.total_weight_accrued);

        if let Some(vault_addr) = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::VaultAddress)
        {
            try_release_vault_funds(env, task_id, &vault_addr);
        }
    }

    env.storage().instance().set(&voted_key, &true);
    storage::append_task_voter(env, task_id, &guardian);
    storage::set_active_task(env, &t);

    events::emit_weighted_vote(env, task_id, &guardian, weight);

    reentrancy::unlock(env);
    Ok(())
}

/// Core vote logic without authentication or reentrancy management.
/// Performs per-task validation and state mutation.
/// The caller must hold the reentrancy lock and have verified guardian-level checks.
pub(crate) fn vote_inner(
    env: &Env,
    guardian: &Address,
    task_id: u64,
    weight: u64,
) -> Result<(), ContractError> {
    let voted_key = DataKey::Voted(task_id, guardian.clone());
    if env.storage().instance().has(&voted_key) {
        return Err(ContractError::DuplicateVote);
    }

    let mut t = match storage::get_active_task(env, task_id) {
        Some(t) => t,
        None => return Err(ContractError::TaskNotFound),
    };

    if t.is_cancelled {
        return Err(ContractError::TaskCancelled);
    }

    let weight_threshold: u64 = env
        .storage()
        .instance()
        .get(&DataKey::WeightThreshold)
        .unwrap_or(DEFAULT_WEIGHT_THRESHOLD);

    // Keep this transition delegated to the Kani-verified consensus module.
    // Reimplementing it here could make the on-chain path diverge from the
    // arithmetic proved in `verification/`; see
    // [VERIFICATION_REPORT.md](../../../docs/history/VERIFICATION_REPORT.md).
    let mut consensus_state = crate::consensus::ConsensusState {
        total_weight_accrued: t.total_weight_accrued,
        votes: t.votes,
        is_done: t.is_done,
    };
    crate::consensus::apply_vote(&mut consensus_state, weight, weight_threshold).map_err(|e| {
        match e {
            crate::consensus::ConsensusError::WeightOverflow => ContractError::WeightOverflow,
            crate::consensus::ConsensusError::ZeroWeight => ContractError::ZeroWeightVote,
        }
    })?;
    t.total_weight_accrued = consensus_state.total_weight_accrued;
    t.votes = consensus_state.votes;

    if consensus_state.is_done && t.votes >= t.min_votes_required && !t.is_done {
        t.is_done = true;
        t.resolved_at = env.ledger().timestamp();
        events::emit_task_resolved(env, task_id, t.total_weight_accrued);

        if let Some(vault_addr) = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::VaultAddress)
        {
            try_release_vault_funds(env, task_id, &vault_addr);
        }
    }

    env.storage().instance().set(&voted_key, &true);
    storage::append_task_voter(env, task_id, guardian);
    storage::set_active_task(env, &t);

    events::emit_weighted_vote(env, task_id, guardian, weight);

    Ok(())
}

/// Vote on multiple tasks in a single atomic transaction.
/// Guardian-level checks (auth, guardian status, balance, reputation) are
/// performed once. Per-task validation and state mutation uses `vote_inner`.
/// If any task is invalid the entire batch is reverted (Soroban transactional
/// semantics ensure atomicity).
pub(crate) fn process_vote_batch(
    env: &Env,
    guardian: Address,
    task_ids: Vec<u64>,
) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    reentrancy::lock(env)?;

    if !guardian::is_guardian(env, &guardian) {
        reentrancy::unlock(env);
        return Err(ContractError::NotAuthorized);
    }

    let token_key = DataKey::TokenAddress;
    if !env.storage().instance().has(&token_key) {
        reentrancy::unlock(env);
        return Err(ContractError::NotInitialized);
    }

    let threshold: i128 = env
        .storage()
        .instance()
        .get(&DataKey::LockThreshold)
        .unwrap_or(0);
    let balance_key = DataKey::LockedBalance(guardian.clone());
    let locked_balance: i128 = env.storage().instance().get(&balance_key).unwrap_or(0);

    if locked_balance <= threshold {
        reentrancy::unlock(env);
        return Err(ContractError::InsufficientLockedBalance);
    }

    let weight = match reputation::get_rep(env, &guardian) {
        Ok(w) => w,
        Err(e) => {
            reentrancy::unlock(env);
            return Err(e);
        }
    };

    if weight == 0 {
        reentrancy::unlock(env);
        return Err(ContractError::ZeroWeightVote);
    }

    for task_id in task_ids.iter() {
        if let Err(e) = vote_inner(env, &guardian, task_id, weight) {
            reentrancy::unlock(env);
            return Err(e);
        }
    }

    reentrancy::unlock(env);
    Ok(())
}

pub(crate) fn get_snapshot(env: &Env) -> Result<Snapshot, ContractError> {
    let timestamp = env.ledger().timestamp();
    let paused = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    let failure_count = env
        .storage()
        .instance()
        .get(&DataKey::FailureCount)
        .unwrap_or(0);
    let weight_threshold = env
        .storage()
        .instance()
        .get(&DataKey::WeightThreshold)
        .unwrap_or(DEFAULT_WEIGHT_THRESHOLD);
    let admin = env.storage().instance().get(&DataKey::Admin);
    let vault_address = env.storage().instance().get(&DataKey::VaultAddress);
    let drips_address = env.storage().instance().get(&DataKey::DripsAddress);

    let all_guardians = guardian::get_all_guardians(env);
    let all_tasks = task::get_all_tasks(env);
    let all_streams = drips::get_all_reward_streams(env);

    // Bail out before doing any of the expensive per-entry work below: once a
    // collection outgrows the ledger's practical per-transaction CPU budget,
    // building the full snapshot atomically is no longer safe. Callers past
    // this point should use the paginated API instead (see
    // `MAX_SNAPSHOT_COLLECTION_SIZE`).
    if all_guardians.len() > limits::MAX_SNAPSHOT_COLLECTION_SIZE
        || all_tasks.len() > limits::MAX_SNAPSHOT_COLLECTION_SIZE
        || all_streams.len() > limits::MAX_SNAPSHOT_COLLECTION_SIZE
    {
        return Err(ContractError::SnapshotTooLarge);
    }

    let mut guardians = Map::new(env);
    for g in all_guardians.iter() {
        guardians.set(g.clone(), guardian::is_guardian(env, &g));
    }

    let mut reputations = Map::new(env);
    for g in all_guardians.iter() {
        if let Some(score) = reputation::get_reputation(env, &g) {
            reputations.set(g.clone(), score);
        }
    }

    let mut tasks = Map::new(env);
    for t in all_tasks.iter() {
        if let Some(task) = task::get_task(env, t) {
            tasks.set(t, task);
        }
    }

    let mut votes = Map::new(env);
    for t in all_tasks.iter() {
        let task_id = t;
        let task_voters = storage::get_task_voters(env, task_id);
        for voter in task_voters.iter() {
            votes.set((task_id, voter.clone()), true);
        }
    }

    let mut reward_streams = Map::new(env);
    for s in all_streams.iter() {
        if let Some(stream) = drips::get_reward_stream(env, s) {
            reward_streams.set(s, stream);
        }
    }

    Ok(Snapshot {
        timestamp,
        paused,
        failure_count,
        weight_threshold,
        admin,
        vault_address,
        drips_address,
        guardians,
        reputations,
        tasks,
        votes,
        reward_streams,
    })
}

/// O(1) snapshot header (plus collection counts). Always safe to call,
/// regardless of total protocol size.
pub(crate) fn get_snapshot_meta(env: &Env) -> SnapshotMeta {
    SnapshotMeta {
        timestamp: env.ledger().timestamp(),
        paused: env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false),
        failure_count: env
            .storage()
            .instance()
            .get(&DataKey::FailureCount)
            .unwrap_or(0),
        weight_threshold: env
            .storage()
            .instance()
            .get(&DataKey::WeightThreshold)
            .unwrap_or(DEFAULT_WEIGHT_THRESHOLD),
        admin: env.storage().instance().get(&DataKey::Admin),
        vault_address: env.storage().instance().get(&DataKey::VaultAddress),
        drips_address: env.storage().instance().get(&DataKey::DripsAddress),
        guardian_count: env
            .storage()
            .instance()
            .get(&DataKey::GuardianIndexCount)
            .unwrap_or(0),
        task_count: env
            .storage()
            .instance()
            .get(&DataKey::TaskIndexCount)
            .unwrap_or(0),
        reward_stream_count: drips::get_all_reward_streams(env).len(),
    }
}

/// Returns up to `limit` (capped at `MAX_PAGE_LIMIT`) guardians starting at
/// `offset`, with their guardian status and reputation.
///
/// Reads the dense `GuardianIndexAt` slot index maintained by
/// `add_guardian`/`remove_guardian` rather than the full `AllGuardians`
/// list, so this does `O(limit)` storage reads — not `O(total guardian
/// count)` — and stays cheaply invokable at guardian counts where
/// `get_snapshot` is capped out entirely (see `MAX_SNAPSHOT_COLLECTION_SIZE`
/// and `tests/snapshot_scaling.rs`). Its absolute cost still carries a mild
/// dependency on total instance-storage size, an inherent property of
/// Soroban's shared-instance-ledger-entry storage model.
pub(crate) fn get_guardians_page(env: &Env, offset: u32, limit: u32) -> Vec<GuardianEntry> {
    let limit = limit.min(limits::MAX_PAGE_LIMIT);
    let count: u32 = env
        .storage()
        .instance()
        .get(&DataKey::GuardianIndexCount)
        .unwrap_or(0);
    let end = offset.saturating_add(limit).min(count);

    let mut page = Vec::new(env);
    let mut i = offset;
    while i < end {
        if let Some(g) = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::GuardianIndexAt(i))
        {
            let is_g = guardian::is_guardian(env, &g);
            let reputation = reputation::get_reputation(env, &g);
            page.push_back(GuardianEntry {
                address: g,
                is_guardian: is_g,
                reputation,
            });
        }
        i += 1;
    }
    page
}

/// Returns up to `limit` (capped at `MAX_PAGE_LIMIT`) tasks starting at
/// `offset`.
///
/// Reads the dense `TaskIndexAt` slot index maintained by
/// `register_tasks`/`purge_task` rather than the full `AllTasks` list, so
/// this does `O(limit)` storage reads — not `O(total task count)` — and
/// stays cheaply invokable at task counts where `get_snapshot` is capped out
/// entirely. See `get_guardians_page` for the same caveat on absolute cost
/// under Soroban's shared-instance-ledger-entry storage model.
pub(crate) fn get_tasks_page(env: &Env, offset: u32, limit: u32) -> Vec<Task> {
    let limit = limit.min(limits::MAX_PAGE_LIMIT);
    let count: u32 = env
        .storage()
        .instance()
        .get(&DataKey::TaskIndexCount)
        .unwrap_or(0);
    let end = offset.saturating_add(limit).min(count);

    let mut page = Vec::new(env);
    let mut i = offset;
    while i < end {
        if let Some(id) = env
            .storage()
            .instance()
            .get::<_, u64>(&DataKey::TaskIndexAt(i))
        {
            if let Some(t) = task::get_task(env, id) {
                page.push_back(t);
            }
        }
        i += 1;
    }
    page
}

/// Returns up to `limit` (capped at `MAX_PAGE_LIMIT`) reward streams starting
/// at `offset`.
pub(crate) fn get_reward_streams_page(env: &Env, offset: u32, limit: u32) -> Vec<RewardStream> {
    let limit = limit.min(limits::MAX_PAGE_LIMIT);
    let all = drips::get_all_reward_streams(env);
    let end = offset.saturating_add(limit).min(all.len());

    let mut page = Vec::new(env);
    let mut i = offset;
    while i < end {
        // `end` is clamped to `all.len()`, but handle a malformed or
        // concurrently changed collection defensively instead of panicking.
        if let Some(id) = all.get(i) {
            if let Some(s) = drips::get_reward_stream(env, id) {
                page.push_back(s);
            }
        }
        i += 1;
    }
    page
}

pub(crate) fn record_snapshot(env: &Env) -> Result<(), ContractError> {
    let snapshot = get_snapshot(env)?;
    let timestamp = snapshot.timestamp;

    let mut all_snapshots: soroban_sdk::Vec<u64> = env
        .storage()
        .instance()
        .get(&DataKey::AllSnapshots)
        .unwrap_or(soroban_sdk::Vec::new(env));
    all_snapshots.push_back(timestamp);
    env.storage()
        .instance()
        .set(&DataKey::AllSnapshots, &all_snapshots);

    env.storage()
        .instance()
        .set(&DataKey::Snapshot(timestamp), &snapshot);
    events::emit_snapshot_recorded(env, timestamp);

    Ok(())
}
