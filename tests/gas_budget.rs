#![cfg(test)]

use soroban_sdk::token::StellarAssetClient as TestTokenClient;
use soroban_sdk::{
    testutils::Address as _,
    Address, Env,
};
use vero_core_contracts::{gas::*, VeroContractClient, Role};

fn setup() -> (Env, Address, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = token.address();

    client.initialize(&admin, &token_addr, &0i128);
    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.grant_role(&admin, &admin, &Role::ConfigManager);

    // Set a dummy vault address to simulate the worst-case cross-contract call path
    let vault = Address::generate(&env);
    client.set_vault_address(&admin, &vault);

    (env, contract_id, admin, token_addr, client)
}

fn add_guardian_with_rep(
    env: &Env,
    client: &VeroContractClient,
    admin: &Address,
    score: u64,
) -> Address {
    let guardian = Address::generate(env);
    client.add_guardian(admin, &guardian);
    client.set_reputation(admin, &guardian, &score);
    guardian
}

#[test]
fn test_gas_budget_register_task() {
    let (env, _, admin, _, client) = setup();
    
    client.register_task(&admin, &1u64, &1u32);
    let cost = env.cost_estimate().budget().cpu_instruction_cost();
    
    assert!(
        cost <= COST_REGISTER_TASK,
        "register_task cost {} exceeds COST_REGISTER_TASK {}",
        cost,
        COST_REGISTER_TASK
    );
}

#[test]
fn test_gas_budget_vote() {
    let (env, _, admin, _, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 500);
    client.set_weight_threshold(&admin, &500);
    client.register_task(&admin, &1u64, &1u32);
    
    client.vote(&guardian, &1u64);
    let cost = env.cost_estimate().budget().cpu_instruction_cost();
    
    assert!(
        cost <= COST_VOTE,
        "vote cost {} exceeds COST_VOTE {}",
        cost,
        COST_VOTE
    );
}

#[test]
fn test_gas_budget_vote_batch() {
    let (env, _, admin, _, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 500);
    client.set_weight_threshold(&admin, &500);
    
    client.register_task(&admin, &1u64, &1u32);
    client.register_task(&admin, &2u64, &1u32);
    client.register_task(&admin, &3u64, &1u32);
    client.register_task(&admin, &4u64, &1u32);
    client.register_task(&admin, &5u64, &1u32);
    
    let votes = soroban_sdk::vec![
        &env,
        1u64,
        2u64,
        3u64,
        4u64,
        5u64,
    ];
    
    client.vote_batch(&guardian, &votes);
    let cost = env.cost_estimate().budget().cpu_instruction_cost();
    
    assert!(
        cost <= COST_VOTE_BATCH,
        "vote_batch cost {} exceeds COST_VOTE_BATCH {}",
        cost,
        COST_VOTE_BATCH
    );
}

#[test]
fn test_gas_budget_lock_tokens() {
    let (env, _, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 500);
    
    let sac = TestTokenClient::new(&env, &token);
    sac.mint(&guardian, &1000);
    
    client.lock_tokens(&guardian, &1000);
    let cost = env.cost_estimate().budget().cpu_instruction_cost();
    
    assert!(
        cost <= COST_LOCK_TOKENS,
        "lock_tokens cost {} exceeds COST_LOCK_TOKENS {}",
        cost,
        COST_LOCK_TOKENS
    );
}
