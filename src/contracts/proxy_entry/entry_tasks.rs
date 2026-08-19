#![allow(missing_docs)]

//! Task registration, lifecycle and voting entrypoints.
//!
//! Part of the `VeroContract` entrypoint surface; see
//! [`crate::contracts::proxy_entry`] for the overall layout.

use super::{VeroContract, VeroContractClient};
use crate::contracts::logic;
use crate::types::ContractError;
use crate::validation::validate_external_address as validate_address;
use crate::{circuit_breaker, events, storage, task};
use soroban_sdk::{contractimpl, Address, Env, Vec};

#[contractimpl]
impl VeroContract {
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
}
