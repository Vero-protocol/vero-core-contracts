#![cfg(test)]

// Covers GitHub issue #140: purge_task must not leave dangling
// RewardStream(task_id) / AllRewardStreams entries once the task
// referenced by that stream has been purged.

mod common;

use common::MockDripsContract;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{testutils::Address as _, Address, Env};
use vero_core_contracts::{Role, VeroContractClient};

const LOCK_THRESHOLD: i128 = 100;

fn setup() -> (Env, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    client.initialize(&admin, &token_addr, &LOCK_THRESHOLD);

    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.grant_role(&admin, &admin, &Role::TaskManager);
    client.grant_role(&admin, &admin, &Role::ConfigManager);
    client.grant_role(&admin, &admin, &Role::TreasuryManager);

    (env, admin, token_addr, client)
}

fn resolve_task(
    env: &Env,
    admin: &Address,
    token: &Address,
    client: &VeroContractClient,
    task_id: u64,
) {
    client.register_task(admin, &task_id, &1u32);

    let guardian = Address::generate(env);
    client.add_guardian(admin, &guardian);
    client.set_reputation(admin, &guardian, &300);
    client.set_weight_threshold(admin, &1);

    StellarAssetClient::new(env, token).mint(&guardian, &101);
    client.lock_tokens(&guardian, &101);
    client.vote(&guardian, &task_id);

    assert!(client.get_task(&task_id).unwrap().is_done);
}

#[test]
fn test_purge_task_removes_reward_stream_and_index_entry() {
    let (env, admin, token, client) = setup();
    let contributor = Address::generate(&env);
    let drips = env.register_contract(None, MockDripsContract);

    resolve_task(&env, &admin, &token, &client, 1u64);

    client.start_reward_stream(&admin, &drips, &contributor, &1u64);
    assert!(client.get_reward_stream(&1u64).is_some());
    assert!(client.get_snapshot().reward_streams.get(1u64).is_some());

    client.purge_task(&admin, &1u64);

    // Task is gone, as before this fix.
    assert!(client.get_task(&1u64).is_none());

    // Documented behavior: reward streams do not outlive their task, so both
    // the direct lookup and the snapshot's index-derived view must be empty.
    assert!(client.get_reward_stream(&1u64).is_none());
    assert!(client.get_snapshot().reward_streams.get(1u64).is_none());
}

#[test]
fn test_purge_task_reward_stream_cleanup_preserves_other_streams() {
    let (env, admin, token, client) = setup();
    let contributor = Address::generate(&env);
    let drips = env.register_contract(None, MockDripsContract);

    resolve_task(&env, &admin, &token, &client, 1u64);
    resolve_task(&env, &admin, &token, &client, 2u64);

    client.start_reward_stream(&admin, &drips, &contributor, &1u64);
    client.start_reward_stream(&admin, &drips, &contributor, &2u64);

    client.purge_task(&admin, &1u64);

    // Purged task's stream is gone.
    assert!(client.get_reward_stream(&1u64).is_none());

    // The other task's stream survives, both directly and in the snapshot.
    assert!(client.get_reward_stream(&2u64).is_some());
    let snapshot = client.get_snapshot();
    assert!(snapshot.reward_streams.get(1u64).is_none());
    assert!(snapshot.reward_streams.get(2u64).is_some());
}
