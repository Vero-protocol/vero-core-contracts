#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};
use vero_core_contracts::{Role, VeroContractClient, ARCHIVE_AFTER_SECONDS};

fn setup() -> (Env, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();
    client.initialize(&admin, &token_addr, &100i128);
    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.grant_role(&admin, &admin, &Role::TaskManager);
    client.grant_role(&admin, &admin, &Role::ConfigManager);
    (env, admin, token_addr, client)
}

#[test]
fn test_non_taskmanager_cannot_archive_task() {
    let (env, admin, token, client) = setup();
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &100u64);
    client.set_weight_threshold(&admin, &1u64);
    client.register_task(&admin, &1u64, &1u32);

    let sac = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    sac.mint(&guardian, &101i128);
    client.lock_tokens(&guardian, &101i128);

    env.ledger().set_timestamp(1_000);
    client.vote(&guardian, &1u64);
    env.ledger()
        .set_timestamp(1_000 + ARCHIVE_AFTER_SECONDS + 1);

    let stranger = Address::generate(&env);
    let result = client.try_archive_task(&stranger, &1u64);
    assert!(result.is_err());
    assert!(client.get_task(&1u64).is_some());
    assert!(client.get_archived_task(&1u64).is_none());
}

#[test]
fn test_taskmanager_can_archive_task() {
    let (env, admin, token, client) = setup();
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &100u64);
    client.set_weight_threshold(&admin, &1u64);
    client.register_task(&admin, &2u64, &1u32);

    let sac = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    sac.mint(&guardian, &101i128);
    client.lock_tokens(&guardian, &101i128);

    env.ledger().set_timestamp(1_000);
    client.vote(&guardian, &2u64);
    env.ledger()
        .set_timestamp(1_000 + ARCHIVE_AFTER_SECONDS + 1);

    client.archive_task(&admin, &2u64);
    assert!(client.get_task(&2u64).is_none());
    assert!(client.get_archived_task(&2u64).is_some());
}
