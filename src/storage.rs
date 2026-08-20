use crate::types::{ContractError, DataKey, Task};
use soroban_sdk::{Address, Env, Vec};

pub const ARCHIVE_AFTER_SECONDS: u64 = 30 * 24 * 60 * 60;

pub fn active_task_key(task_id: u64) -> DataKey {
    DataKey::Task(task_id)
}

pub fn archived_task_key(task_id: u64) -> DataKey {
    DataKey::ArchivedTask(task_id)
}

#[allow(dead_code)]
pub fn has_active_task(env: &Env, task_id: u64) -> bool {
    env.storage().instance().has(&active_task_key(task_id))
}

pub fn get_active_task(env: &Env, task_id: u64) -> Option<Task> {
    env.storage().instance().get(&active_task_key(task_id))
}

pub fn set_active_task(env: &Env, task: &Task) {
    env.storage()
        .instance()
        .set(&active_task_key(task.id), task);
}

pub fn get_archived_task(env: &Env, task_id: u64) -> Option<Task> {
    env.storage().instance().get(&archived_task_key(task_id))
}

pub fn archive_task(env: &Env, task_id: u64) -> Result<(), ContractError> {
    if get_archived_task(env, task_id).is_some() {
        return Err(ContractError::TaskAlreadyArchived);
    }

    let task = get_active_task(env, task_id).ok_or(ContractError::TaskNotFound)?;
    if !task.is_done || task.resolved_at == 0 {
        return Err(ContractError::TaskNotVerified);
    }

    let age = env.ledger().timestamp().saturating_sub(task.resolved_at);
    if age <= ARCHIVE_AFTER_SECONDS {
        return Err(ContractError::TaskNotStale);
    }

    env.storage()
        .instance()
        .set(&archived_task_key(task_id), &task);
    env.storage().instance().remove(&active_task_key(task_id));

    Ok(())
}

pub fn task_voters_key(task_id: u64) -> DataKey {
    DataKey::TaskVoters(task_id)
}

pub fn get_task_voters(env: &Env, task_id: u64) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&task_voters_key(task_id))
        .unwrap_or(Vec::new(env))
}

pub fn append_task_voter(env: &Env, task_id: u64, voter: &Address) {
    let mut voters = get_task_voters(env, task_id);
    voters.push_back(voter.clone());
    env.storage()
        .instance()
        .set(&task_voters_key(task_id), &voters);
}
