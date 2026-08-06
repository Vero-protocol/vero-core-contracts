#![allow(missing_docs)]

use soroban_sdk::{symbol_short, Address, Env};

// `task_id`/`weight` are validated as `u64` in validation.rs and can
// exceed `u32::MAX`. This guard fails to compile if either constant is
// ever redefined to a type that can't round-trip through the `u64`s below.
const _: () = {
    let _: u64 = crate::validation::MAX_TASK_ID;
    let _: u64 = crate::validation::MAX_WEIGHT_THRESHOLD;
};

/// Emits an event when a task reaches consensus.
/// `task_id`/`weight` are carried as full `u64` values (see #159) — no
/// packing/truncation, so ids and weights above `u32::MAX` stay intact.
pub fn emit_task_resolved(env: &Env, task_id: u64, total_weight: u64) {
    env.events()
        .publish((symbol_short!("resolved"),), (task_id, total_weight));
}

/// Emits an event when a guardian casts a weighted vote.
/// `task_id`/`weight` are carried as full `u64` values (see #159) — no
/// packing/truncation, so ids and weights above `u32::MAX` stay intact.
pub fn emit_weighted_vote(env: &Env, task_id: u64, guardian: &Address, weight: u64) {
    env.events().publish(
        (symbol_short!("wt_vote"),),
        (guardian.clone(), task_id, weight),
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Events as _};
    use soroban_sdk::TryIntoVal;

    // Regression test for #159: task_id/weight above u32::MAX must reach
    // the event log unchanged instead of being silently truncated.
    #[test]
    fn large_task_id_and_weight_survive_resolved_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, crate::VeroContract);
        let big_task_id: u64 = (u32::MAX as u64) + 12345;
        let big_weight: u64 = (u32::MAX as u64) + 67890;

        env.as_contract(&contract_id, || {
            emit_task_resolved(&env, big_task_id, big_weight);
        });

        let published = env.events().all();
        let (_contract, _topics, data) = published.last().unwrap();
        let (task_id, weight): (u64, u64) = data.try_into_val(&env).unwrap();
        assert_eq!(task_id, big_task_id);
        assert_eq!(weight, big_weight);
    }

    #[test]
    fn large_task_id_and_weight_survive_weighted_vote_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, crate::VeroContract);
        let guardian = Address::generate(&env);
        let big_task_id: u64 = (u32::MAX as u64) + 1;
        let big_weight: u64 = (u32::MAX as u64) + 1;

        env.as_contract(&contract_id, || {
            emit_weighted_vote(&env, big_task_id, &guardian, big_weight);
        });

        let published = env.events().all();
        let (_contract, _topics, data) = published.last().unwrap();
        let (g, task_id, weight): (Address, u64, u64) = data.try_into_val(&env).unwrap();
        assert_eq!(g, guardian);
        assert_eq!(task_id, big_task_id);
        assert_eq!(weight, big_weight);
    }
}
