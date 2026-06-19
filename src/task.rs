use soroban_sdk::{Address, Env, Vec};

use crate::events;
use crate::reentrancy;
use crate::storage;
use crate::types::{ContractError, Task};
use crate::validation;

/// A task is "terminal" when it has been fully resolved or explicitly cancelled.
/// Only terminal tasks may be purged.
fn is_terminal(task: &Task) -> bool {
    task.is_done || task.is_cancelled
}

const MAX_REGISTER_TASK_BATCH_SIZE: u32 = 32;

pub fn register_tasks(env: &Env, admin: Address, task_ids: Vec<u64>) -> Result<(), ContractError> {
    if task_ids.is_empty() || task_ids.len() > MAX_REGISTER_TASK_BATCH_SIZE {
        return Err(ContractError::BatchTooLarge);
    }

    validation::validate_admin_address(env, &admin)?;
    admin.require_auth();

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
        .get(&storage::all_tasks_key())
        .unwrap_or(Vec::new(env));

    for task_id in task_ids.iter() {
        if storage::get_active_task(env, task_id).is_some() {
            reentrancy::unlock(env);
            return Err(ContractError::NotAuthorized);
        }

        all_tasks.push_back(task_id);

        let task = Task {
            id: task_id,
            votes: 0,
            is_done: false,
            resolved_at: 0,
            total_weight_accrued: 0,
            is_cancelled: false,
        };
        storage::set_active_task(env, &task);
        all_tasks.push_back(task_id);
    }

    env.storage()
        .instance()
        .set(&storage::all_tasks_key(), &all_tasks);

    reentrancy::unlock(env);
    Ok(())
}

pub fn cancel_task(env: &Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
    validation::validate_admin_address(env, &admin)?;
    admin.require_auth();
    validation::validate_task_id(task_id)?;

    let mut task = storage::get_active_task(env, task_id).ok_or(ContractError::TaskNotFound)?;
    if task.is_cancelled {
        return Err(ContractError::TaskCancelled);
    }

    task.is_cancelled = true;
    storage::set_active_task(env, &task);
    events::emit_task_cancelled(env, task_id);
    Ok(())
}

pub fn get_task(env: &Env, task_id: u64) -> Option<Task> {
    storage::get_active_task(env, task_id)
}

pub fn get_all_tasks(env: &Env) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&storage::all_tasks_key())
        .unwrap_or(Vec::new(env))
}

/// Purge a terminal task (done or cancelled) from contract storage.
///
/// Removes:
/// - the active task key — the live task entry
/// - the archived task key — the archived copy, if one exists
/// - the task voters key — the per-task voter list
/// - the voted key — each individual vote record
/// - The task_id entry in the `AllTasks` index
///
/// Reverts with `TaskNotFound` when no active or archived task exists for the
/// given id. Reverts with `TaskNotTerminal` when the task is still active
/// (neither done nor cancelled).
///
/// Admin authentication is required.
pub fn purge_task(env: &Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
    admin.require_auth();

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
            .remove(&storage::voted_key(task_id, voter.clone()));
    }
    env.storage()
        .instance()
        .remove(&storage::task_voters_key(task_id));

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
        .get(&storage::all_tasks_key())
        .unwrap_or(Vec::new(env));
    let mut updated = Vec::new(env);
    for id in all_tasks.iter() {
        if id != task_id {
            updated.push_back(id);
        }
    }
    env.storage()
        .instance()
        .set(&storage::all_tasks_key(), &updated);

    events::emit_task_purged(env, task_id);

    Ok(())
}
