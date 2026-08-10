#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient as TestTokenClient;
use soroban_sdk::{Address, Env};
use vero_core_contracts::{DataKey, Role, Task, VeroContractClient};

fn setup() -> (Env, Address, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    client.initialize(&admin, &token, &0);
    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.grant_role(&admin, &admin, &Role::TaskManager);

    (env, contract_id, admin, token, client)
}

fn add_voter(env: &Env, client: &VeroContractClient, admin: &Address, token: &Address) -> Address {
    let guardian = Address::generate(env);
    client.add_guardian(admin, &guardian);
    client.set_reputation(admin, &guardian, &100);
    TestTokenClient::new(env, token).mint(&guardian, &1);
    client.lock_tokens(&guardian, &1);
    guardian
}

#[test]
fn vote_uses_verified_consensus_boundaries() {
    let (env, contract_id, admin, token, client) = setup();
    let guardian = add_voter(&env, &client, &admin, &token);

    client.register_task(&admin, &1, &1);
    let mut task: Task = client.get_task(&1).unwrap();
    task.votes = u32::MAX;
    // Seed the persisted task at the boundary, then exercise the public vote
    // entry point so authentication, storage, and consensus wiring all run.
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Task(1), &task);
    });

    client.vote(&guardian, &1);
    assert_eq!(client.get_task(&1).unwrap().votes, u32::MAX);

    client.register_task(&admin, &2, &1);
    let mut task: Task = client.get_task(&2).unwrap();
    task.total_weight_accrued = u64::MAX;
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Task(2), &task);
    });

    assert!(client.try_vote(&guardian, &2).is_err());
    let task = client.get_task(&2).unwrap();
    assert_eq!(task.total_weight_accrued, u64::MAX);
    assert_eq!(task.votes, 0);

    client.register_task(&admin, &3, &1);
    let mut task: Task = client.get_task(&3).unwrap();
    task.total_weight_accrued = 300;
    task.is_done = true;
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Task(3), &task);
    });

    client.vote(&guardian, &3);
    assert!(client.get_task(&3).unwrap().is_done);
}
