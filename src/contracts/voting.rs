//! Vote processing: single-vote, inner (lock-held) vote, and batch voting.
//!
//! This module owns the guardian vote path — auth, balance/reputation checks,
//! reentrancy locking, consensus application, and vault-release on resolution.
//! Token/fee vault operations live in [`super::vault_ops`]; snapshot/pagination
//! lives in [`super::snapshot`].

use crate::types::{ContractError, DataKey};
use crate::DEFAULT_WEIGHT_THRESHOLD;
use crate::{circuit_breaker, events, guardian, reentrancy, reputation, storage};
use soroban_sdk::{contractclient, Address, Env, Vec};

#[contractclient(name = "VaultClient")]
#[allow(dead_code)]
pub trait Vault {
    fn release_funds(env: Env, task_id: u64);
}

/// Attempts to release funds from the vault.  If the vault call fails the
/// failure is logged via an event but does not revert the transaction; task
/// resolution must never be blocked by a broken vault.
pub(crate) fn try_release_vault_funds(env: &Env, task_id: u64, vault_addr: &Address) {
    let vault_client = VaultClient::new(env, vault_addr);
    let result = vault_client.try_release_funds(&task_id);

    match result {
        Ok(_) => {
            events::emit_vault_release_success(env, task_id);
        }
        Err(_e) => {
            events::emit_vault_release_failed(env, task_id);
        }
    }
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