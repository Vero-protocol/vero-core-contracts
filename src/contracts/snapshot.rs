//! Snapshot building and paginated collection views.
//!
//! This module owns the functions that read contract state without mutating
//! it: the full atomic snapshot, the O(1) metadata header, and the three
//! paginated views (guardians, tasks, reward streams).  Token/fee vault
//! operations live in [`super::vault_ops`]; vote processing lives in
//! [`super::voting`].

use crate::types::{ContractError, DataKey, GuardianEntry, RewardStream, Snapshot, SnapshotMeta, Task};
use crate::DEFAULT_WEIGHT_THRESHOLD;
use crate::{drips, events, guardian, limits, reputation, storage, task};
use soroban_sdk::{Address, Env, Map, Vec};

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
        let id = all.get(i).unwrap();
        if let Some(s) = drips::get_reward_stream(env, id) {
            page.push_back(s);
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
