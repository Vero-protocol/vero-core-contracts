use soroban_sdk::{symbol_short, Address, Env};

pub fn emit_task_resolved(env: &Env, task_id: u64, weight: u64) {
    env.events()
        .publish((symbol_short!("resolved"),), (task_id, weight));
}

pub fn emit_weighted_vote(env: &Env, task_id: u64, guardian: &Address, weight: u64) {
    env.events().publish(
        (symbol_short!("wt_vote"),),
        (task_id, guardian.clone(), weight),
    );
}

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

pub fn emit_circuit_breaker_triggered(env: &Env, failure_count: u32) {
    env.events()
        .publish((symbol_short!("cb_trip"),), (failure_count,));
}

pub fn emit_withdrawal_requested(env: &Env, request_id: u64, recipient: &Address, amount: i128) {
    env.events().publish(
        (symbol_short!("wd_req"),),
        (request_id, recipient.clone(), amount),
    );
}

pub fn emit_withdrawal_executed(env: &Env, request_id: u64, recipient: &Address, amount: i128) {
    env.events().publish(
        (symbol_short!("wd_exec"),),
        (request_id, recipient.clone(), amount),
    );
}

pub fn emit_withdrawal_cancelled(env: &Env, request_id: u64) {
    env.events()
        .publish((symbol_short!("wd_cncl"),), request_id);
}

pub fn emit_task_cancelled(env: &Env, task_id: u64) {
    env.events().publish((symbol_short!("cancelled"),), task_id);
}

pub fn emit_snapshot_recorded(env: &Env, timestamp: u64) {
    env.events()
        .publish((symbol_short!("snapshot"),), timestamp);
}

pub fn emit_task_purged(env: &Env, task_id: u64) {
    env.events().publish((symbol_short!("purged"),), task_id);
}
