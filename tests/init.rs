#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use vero_core_contracts::VeroContractClient;

#[test]
fn test_registry_starts_clean() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    assert!(client.get_task(&1u64).is_none());
    assert!(client.get_reward_stream(&1u64).is_none());
    assert_eq!(client.get_weight_threshold(), 300);

    let stranger = Address::generate(&env);
    assert_eq!(client.calculate_voting_power(&stranger), None);
    assert_eq!(client.get_reputation(&stranger), None);
}

#[test]
fn test_reinitialize_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    client.initialize(&admin, &token_addr, &100i128);

    let result = client.try_initialize(&admin, &token_addr, &100i128);
    assert!(result.is_err(), "second initialize() must revert");
}

#[test]
fn test_admin_stored_on_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);

    client.initialize(&admin, &token.address(), &100i128);

    assert_eq!(client.get_admin(), Some(admin));
}
