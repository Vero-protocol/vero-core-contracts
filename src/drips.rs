#![allow(missing_docs)]

use soroban_sdk::{Address, Env, IntoVal, Symbol, Val, Vec as SorobanVec};

use crate::storage;
use crate::types::{ContractError, DataKey, RewardStream};
use crate::validation;

/// Starts a Drips reward stream for a verified task to reward a contributor.
pub fn start_drips_stream(
    env: &Env,
    drips_address: Address,
    contributor: Address,
    task_id: u64,
) -> Result<(), ContractError> {
    validation::validate_reward_stream_config(env, &drips_address, &contributor, task_id)?;
    let task = storage::get_active_task(env, task_id).ok_or(ContractError::TaskNotFound)?;

    if task.is_cancelled {
        return Err(ContractError::TaskCancelled);
    }

    if !task.is_done {
        return Err(ContractError::TaskNotVerified);
    }

    let stream_key = DataKey::RewardStream(task_id);
    if env.storage().instance().has(&stream_key) {
        return Err(ContractError::StreamAlreadyActive);
    }

    let resolution_status: u32 = 1;
    let args: SorobanVec<Val> = SorobanVec::from_array(
        env,
        [
            contributor.clone().into_val(env),
            task_id.into_val(env),
            resolution_status.into_val(env),
        ],
    );

    env.invoke_contract::<Val>(&drips_address, &Symbol::new(env, "start_stream"), args);

    let stream = RewardStream {
        task_id,
        contributor: contributor.clone(),
        drips_contract: drips_address,
        active: true,
    };
    env.storage().instance().set(&stream_key, &stream);

    let mut all_streams: SorobanVec<u64> = env
        .storage()
        .instance()
        .get(&DataKey::AllRewardStreams)
        .unwrap_or(SorobanVec::new(env));
    all_streams.push_back(task_id);
    env.storage()
        .instance()
        .set(&DataKey::AllRewardStreams, &all_streams);

    Ok(())
}

/// Retrieves the reward stream details for a specific task.
pub fn get_reward_stream(env: &Env, task_id: u64) -> Option<RewardStream> {
    env.storage()
        .instance()
        .get(&DataKey::RewardStream(task_id))
}

/// Retrieves a list of all active reward stream task IDs.
pub fn get_all_reward_streams(env: &Env) -> SorobanVec<u64> {
    env.storage()
        .instance()
        .get(&DataKey::AllRewardStreams)
        .unwrap_or(SorobanVec::new(env))
}
