#![allow(missing_docs)]

use soroban_sdk::{symbol_short, Address, Env};

/// Compact bitmask for weighted vote events.
/// Bits 0-31: task_id (u32)
/// Bits 32-63: weight (u32, truncated if > u32::MAX)
/// This reduces storage from 2 u64 values to a single u64.
#[inline]
fn pack_vote_data(task_id: u64, weight: u64) -> u64 {
    let tid = (task_id & 0xFFFF_FFFF) as u32;
    let w = (weight.min(0xFFFF_FFFF_u64)) as u32;
    ((w as u64) << 32) | (tid as u64)
}

/// Emits an event when a task reaches consensus.
/// Uses compact format: packs task_id and weight into single u64.
pub fn emit_task_resolved(env: &Env, task_id: u64, total_weight: u64) {
    let packed = pack_vote_data(task_id, total_weight);
    env.events().publish((symbol_short!("resolved"),), packed);
}

/// Emits an event when a guardian casts a weighted vote.
/// Uses compact format: packs task_id and weight into single u64.
pub fn emit_weighted_vote(env: &Env, task_id: u64, guardian: &Address, weight: u64) {
    let packed = pack_vote_data(task_id, weight);
    env.events()
        .publish((symbol_short!("wt_vote"),), (guardian.clone(), packed));
}

/// Emits an event when the pause state is toggled.
pub fn emit_pause_toggled(env: &Env, paused: bool) {
    env.events().publish((symbol_short!("paused"),), paused);
}

pub fn emit_reward_stream_started(env: &Env, task_id: u64, contributor: &Address) {
    env.events()
        .publish((symbol_short!("rw_start"),), (task_id, contributor.clone()));
}

pub fn emit_reward_stream_failed(env: &Env, task_id: u64, contributor: &Address) {
    env.events()
        .publish((symbol_short!("rw_fail"),), (task_id, contributor.clone()));
}

/// Emits an event when the circuit breaker trips and pauses the contract.
///
/// Event topic: `"cb_trip"` (circuit_breaker_triggered)
/// Event data: `failure_count`
pub fn emit_circuit_breaker_triggered(env: &Env, failure_count: u32) {
    env.events()
        .publish((symbol_short!("cb_trip"),), failure_count);
}

/// Emits an event when an authenticated observer reports a failure.
///
/// Event topic: `"cb_report"` (failure_reported)
/// Event data: `(reporter, new_failure_count)`
pub fn emit_failure_reported(env: &Env, reporter: &Address, failure_count: u32) {
    env.events()
        .publish((symbol_short!("cb_report"),), (reporter.clone(), failure_count));
}

/// Emits an event when "trusted reporters only" mode is toggled.
///
/// Event topic: `"cb_trust"`
/// Event data: `(admin, enabled)`
pub fn emit_trusted_reporters_only_set(env: &Env, admin: &Address, enabled: bool) {
    env.events()
        .publish((symbol_short!("cb_trust"),), (admin.clone(), enabled));
}

pub fn emit_role_granted(env: &Env, caller: &Address, target: &Address, role: u8) {
    // Pack role into u32 for Soroban SDK compatibility
    env.events().publish(
        (symbol_short!("role_gr"),),
        (caller.clone(), target.clone(), role as u32),
    );
}

pub fn emit_role_revoked(env: &Env, caller: &Address, target: &Address, role: u8) {
    // Pack role into u32 for Soroban SDK compatibility
    env.events().publish(
        (symbol_short!("role_rv"),),
        (caller.clone(), target.clone(), role as u32),
    );
}

pub fn emit_task_cancelled(env: &Env, task_id: u64) {
    env.events().publish((symbol_short!("cancel"),), task_id);
}

pub fn emit_task_purged(env: &Env, task_id: u64) {
    env.events().publish((symbol_short!("purged"),), task_id);
}

pub fn emit_contract_initialized(env: &Env, admin: &Address) {
    env.events()
        .publish((symbol_short!("inited"),), (admin.clone(),));
}

pub fn emit_guardian_added(env: &Env, admin: &Address, guardian: &Address) {
    env.events().publish(
        (symbol_short!("gd_add"),),
        (admin.clone(), guardian.clone()),
    );
}

pub fn emit_guardian_removed(env: &Env, admin: &Address, guardian: &Address) {
    env.events()
        .publish((symbol_short!("gd_rm"),), (admin.clone(), guardian.clone()));
}

pub fn emit_reputation_set(env: &Env, admin: &Address, guardian: &Address, score: u64) {
    env.events().publish(
        (symbol_short!("rep_set"),),
        (admin.clone(), guardian.clone(), score),
    );
}

pub fn emit_tokens_locked(env: &Env, guardian: &Address, amount: i128) {
    env.events()
        .publish((symbol_short!("tk_lock"),), (guardian.clone(), amount));
}

pub fn emit_timelock_started(env: &Env, guardian: &Address) {
    env.events()
        .publish((symbol_short!("tm_start"),), (guardian.clone(),));
}

pub fn emit_tokens_unlocked(env: &Env, guardian: &Address, amount: i128) {
    env.events()
        .publish((symbol_short!("tk_unlk"),), (guardian.clone(), amount));
}

pub fn emit_emergency_recovery(env: &Env, admin: &Address, recipient: &Address, amount: i128) {
    env.events().publish(
        (symbol_short!("em_rec"),),
        (admin.clone(), recipient.clone(), amount),
    );
}

pub fn emit_guardian_resigned(env: &Env, guardian: &Address) {
    env.events()
        .publish((symbol_short!("gd_res"),), (guardian.clone(),));
}

pub fn emit_threshold_set(env: &Env, admin: &Address, threshold: u64) {
    env.events()
        .publish((symbol_short!("th_set"),), (admin.clone(), threshold));
}

pub fn emit_vault_set(env: &Env, admin: &Address, vault: &Address) {
    env.events()
        .publish((symbol_short!("vault"),), (admin.clone(), vault.clone()));
}

pub fn emit_task_registered(env: &Env, admin: &Address, task_id: u64) {
    env.events()
        .publish((symbol_short!("reg"),), (admin.clone(), task_id));
}

pub fn emit_task_archived(env: &Env, task_id: u64) {
    env.events().publish((symbol_short!("archived"),), task_id);
}

pub fn emit_circuit_breaker_reset(env: &Env, admin: &Address) {
    env.events()
        .publish((symbol_short!("cb_rst"),), (admin.clone(),));
}

pub fn emit_contract_upgraded(env: &Env, admin: &Address, wasm_hash: &soroban_sdk::BytesN<32>) {
    env.events().publish(
        (symbol_short!("upgraded"),),
        (admin.clone(), wasm_hash.clone()),
    );
}

pub fn emit_upgrade_signers_set(env: &Env, signer_count: u32, threshold: u32) {
    env.events()
        .publish((symbol_short!("up_sig"),), (signer_count, threshold));
}

pub fn emit_upgrade_proposed(env: &Env, signer: &Address) {
    env.events()
        .publish((symbol_short!("up_prop"),), (signer.clone(),));
}

pub fn emit_upgrade_approved(env: &Env, signer: &Address, count: u32, threshold: u32) {
    env.events().publish(
        (symbol_short!("up_app"),),
        (signer.clone(), count, threshold),
    );
}

pub fn emit_upgrade_executed(env: &Env) {
    env.events().publish((symbol_short!("up_exec"),), ());
}

pub fn emit_upgrade_cancelled(env: &Env) {
    env.events().publish((symbol_short!("up_cncl"),), ());
}

pub fn emit_snapshot_recorded(env: &Env, timestamp: u64) {
    env.events().publish((symbol_short!("snap"),), timestamp);
}
