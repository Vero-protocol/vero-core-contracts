#![cfg(test)]

//! Tests to ensure that the actual CPU instruction budget consumption of
//! high-traffic operations does not exceed the conservative estimates
//! documented in `src/gas.rs`. This prevents regressions where refactoring
//! could inadvertently push real costs above the hand-estimated limits,
//! causing callers using `get_estimated_cost` to under-fund transactions.

use soroban_sdk::token::StellarAssetClient as TestTokenClient;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};
use vero_core_contracts::{gas::*, Role, VeroContractClient};

/// Asserts that the CPU instructions consumed by the most recent contract
/// invocation on the environment do not exceed the specified maximum cost.
/// Note: `env.cost_estimate().budget()` resets before every top-level invocation.
macro_rules! assert_budget_limit {
    ($env:expr, $max_cost:expr, $op_name:expr) => {
        let cost = $env.cost_estimate().budget().cpu_instruction_cost();
        assert!(
            cost <= $max_cost,
            "{} cost ({}) exceeds documented limit ({})",
            $op_name,
            cost,
            $max_cost
        );
    };
}

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

    // Set a dummy vault address to simulate the worst-case cross-contract call path.
    // This is important because the cost estimates in `gas.rs` account for this.
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
    assert_budget_limit!(env, COST_REGISTER_TASK, "register_task");
}

#[test]
fn test_gas_budget_vote() {
    let (env, _, admin, _, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 500);
    client.set_weight_threshold(&admin, &500);
    client.register_task(&admin, &1u64, &1u32);
    
    client.vote(&guardian, &1u64);
    assert_budget_limit!(env, COST_VOTE, "vote");
}

#[test]
fn test_gas_budget_vote_batch() {
    let (env, _, admin, _, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 500);
    client.set_weight_threshold(&admin, &500);
    
    let mut votes = Vec::new(&env);
    for task_id in 1..=5 {
        client.register_task(&admin, &task_id, &1u32);
        votes.push_back(task_id);
    }
    
    client.vote_batch(&guardian, &votes);
    assert_budget_limit!(env, COST_VOTE_BATCH, "vote_batch");
}

#[test]
fn test_gas_budget_lock_tokens() {
    let (env, _, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 500);
    
    let sac = TestTokenClient::new(&env, &token);
    sac.mint(&guardian, &1000);
    
    client.lock_tokens(&guardian, &1000);
    assert_budget_limit!(env, COST_LOCK_TOKENS, "lock_tokens");
}
