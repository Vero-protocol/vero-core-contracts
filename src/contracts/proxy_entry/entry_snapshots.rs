#![allow(missing_docs)]

//! Snapshot and paginated-view entrypoints.
//!
//! Part of the `VeroContract` entrypoint surface; see
//! [`crate::contracts::proxy_entry`] for the overall layout.

use super::{VeroContract, VeroContractClient};
use crate::circuit_breaker;
use crate::contracts::logic;
use crate::types::{
    ContractError, DataKey, GuardianEntry, RewardStream, Snapshot, SnapshotMeta, Task,
};
use soroban_sdk::{contractimpl, Env, Vec};

#[contractimpl]
impl VeroContract {
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
}
