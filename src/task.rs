#![allow(missing_docs)]

use soroban_sdk::{panic_with_error, Address, Env, Vec};

use crate::events;
use crate::reentrancy;
use crate::storage;
use crate::types::{ContractError, DataKey, Error, Task};
use crate::validation;

/// A task is "terminal" when it has been fully resolved or explicitly cancelled.
/// Only terminal tasks may be purged.
fn is_terminal(task: &Task) -> bool {
    task.is_done || task.is_cancelled
}

const MAX_REGISTER_TASK_BATCH_SIZE: u32 = 32;

/// Registers a batch of new voting tasks in the contract storage.
///
/// Address validation (`admin`) is performed by the calling entrypoint;
/// this helper must not repeat it.
pub fn register_tasks(
    env: &Env,
    admin: Address,
    task_ids: Vec<u64>,
    min_votes_required: u32,
) -> Result<(), ContractError> {
    if task_ids.is_empty() || task_ids.len() > MAX_REGISTER_TASK_BATCH_SIZE {
        return Err(ContractError::BatchTooLarge);
    }

    let mut seen_task_ids = Vec::new(env);
    for task_id in task_ids.iter() {
        validation::validate_task_id(task_id)?;
        if seen_task_ids.contains(task_id) {
            return Err(ContractError::InvalidConfig);
        }
        if storage::has_active_task(env, task_id)
            || storage::get_archived_task(env, task_id).is_some()
        {
            return Err(ContractError::InvalidConfig);
        }
        seen_task_ids.push_back(task_id);
    }

    reentrancy::lock(env)?;

    let mut all_tasks: Vec<u64> = env
        .storage()
        .instance()
        .get(&DataKey::AllTasks)
        .unwrap_or(Vec::new(env));

    for task_id in task_ids.iter() {
        if storage::get_active_task(env, task_id).is_some() {
            reentrancy::unlock(env);
            return Err(ContractError::NotAuthorized);
        }

        let task = Task {
            id: task_id,
            votes: 0,
            is_done: false,
            resolved_at: 0,
            total_weight_accrued: 0,
            is_cancelled: false,
            min_votes_required,
        };
        storage::set_active_task(env, &task);
        all_tasks.push_back(task_id);

        // Maintain a dense slot index alongside `AllTasks` so paginated reads
        // (`get_tasks_page`) can fetch a bounded page of slots instead of the
        // whole task set. See `purge_task` for the matching swap-remove
        // compaction.
        let slot: u32 = env
            .storage()
            .instance()
            .get(&DataKey::TaskIndexCount)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TaskIndexAt(slot), &task_id);
        env.storage()
            .instance()
            .set(&DataKey::TaskIndexOf(task_id), &slot);
        env.storage()
            .instance()
            .set(&DataKey::TaskIndexCount, &(slot + 1));

        events::emit_task_registered(env, &admin, task_id);
    }

    env.storage().instance().set(&DataKey::AllTasks, &all_tasks);

    reentrancy::unlock(env);
    Ok(())
}

/// Cancels an active task.
///
/// Address validation (`admin`) is performed by the calling entrypoint;
/// this helper must not repeat it.
pub fn cancel_task(env: &Env, _admin: Address, task_id: u64) -> Result<(), ContractError> {
    validation::validate_task_id(task_id)?;

    let mut task = storage::get_active_task(env, task_id).ok_or(ContractError::TaskNotFound)?;
    if task.is_done {
        panic_with_error!(env, Error::TaskAlreadyResolved);
    }
    if task.is_cancelled {
        return Err(ContractError::TaskCancelled);
    }

    task.is_cancelled = true;
    storage::set_active_task(env, &task);
    events::emit_task_cancelled(env, task_id);
    Ok(())
}

/// Retrieves an active task from storage by its ID.
pub fn get_task(env: &Env, task_id: u64) -> Option<Task> {
    storage::get_active_task(env, task_id)
}

/// Retrieves all task IDs currently tracked by the contract.
pub fn get_all_tasks(env: &Env) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&DataKey::AllTasks)
        .unwrap_or(Vec::new(env))
}

/// Purge a terminal task (done or cancelled) from contract storage.
///
/// Removes:
/// - `ActiveTask(task_id)` — the live task entry
/// - `ArchivedTask(task_id)` — the archived copy, if one exists
/// - `TaskVoters(task_id)` — the per-task voter list
/// - `Voted(task_id, voter)` — each individual vote record
/// - The task_id entry in the `AllTasks` index
/// - `RewardStream(task_id)` — the task's reward stream record, if one exists
/// - The task_id entry in the `AllRewardStreams` index
///
/// Reward stream records are not retained after their task is purged: a
/// stream is scoped to the lifetime of the task it rewards, so
/// `get_reward_stream`/`get_all_reward_streams`/`get_snapshot` never
/// reference a task_id that no longer exists in `AllTasks`.
///
/// Reverts with `TaskNotFound` when no active or archived task exists for the
/// given id. Reverts with `TaskNotTerminal` when the task is still active
/// (neither done nor cancelled).
///
/// Admin authentication is required.
pub fn purge_task(env: &Env, _admin: Address, task_id: u64) -> Result<(), ContractError> {
    // Resolve from active storage first, then fall back to archived.
    let task = storage::get_active_task(env, task_id)
        .or_else(|| storage::get_archived_task(env, task_id))
        .ok_or(ContractError::TaskNotFound)?;

    // Gate: only terminal tasks may be purged.
    if !is_terminal(&task) {
        return Err(ContractError::TaskNotTerminal);
    }

    // 1. Remove per-voter Voted records then the voters list itself.
    let voters = storage::get_task_voters(env, task_id);
    for voter in voters.iter() {
        env.storage()
            .instance()
            .remove(&DataKey::Voted(task_id, voter.clone()));
    }
    env.storage()
        .instance()
        .remove(&DataKey::TaskVoters(task_id));

    // 2. Remove the task entry from whichever storage slot holds it.
    env.storage()
        .instance()
        .remove(&storage::active_task_key(task_id));
    env.storage()
        .instance()
        .remove(&storage::archived_task_key(task_id));

    // 3. Remove task_id from the AllTasks index.
    let all_tasks: Vec<u64> = env
        .storage()
        .instance()
        .get(&DataKey::AllTasks)
        .unwrap_or(Vec::new(env));
    let mut updated = Vec::new(env);
    for id in all_tasks.iter() {
        if id != task_id {
            updated.push_back(id);
        }
    }
    env.storage().instance().set(&DataKey::AllTasks, &updated);

    // 4. Remove the reward stream record (if any) and its index entry.
    env.storage()
        .instance()
        .remove(&DataKey::RewardStream(task_id));

    let all_streams: Vec<u64> = env
        .storage()
        .instance()
        .get(&DataKey::AllRewardStreams)
        .unwrap_or(Vec::new(env));
    let mut updated_streams = Vec::new(env);
    for id in all_streams.iter() {
        if id != task_id {
            updated_streams.push_back(id);
        }
    }
    env.storage()
        .instance()
        .set(&DataKey::AllRewardStreams, &updated_streams);

    events::emit_task_purged(env, task_id);

    Ok(())
}
