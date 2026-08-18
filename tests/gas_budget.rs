#![cfg(test)]

//! Tests to ensure that the actual CPU instruction budget consumption of
//! high-traffic operations does not exceed the conservative estimates
//! documented in `src/gas.rs`. This prevents regressions where refactoring
//! could inadvertently push real costs above the hand-estimated limits,
//! causing callers using `get_estimated_cost` to under-fund transactions.

use soroban_sdk::token::StellarAssetClient as TestTokenClient;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};
use vero_core_contracts::{gas::*, BatchCall, ContractError, Role, VeroContractClient};

/// Asserts that the CPU instructions consumed by the most recent contract
/// invocation on the environment do not exceed the specified maximum cost.
/// Note: `env.budget()` resets before every top-level invocation.
macro_rules! assert_budget_limit {
    ($env:expr, $max_cost:expr, $op_name:expr) => {
        let cost = $env.budget().cpu_instruction_cost();
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
    client.grant_role(&admin, &admin, &Role::TaskManager);

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
    token: &Address,
    score: u64,
) -> Address {
    let guardian = Address::generate(env);
    client.add_guardian(admin, &guardian);
    client.set_reputation(admin, &guardian, &score);
    TestTokenClient::new(env, token).mint(&guardian, &1);
    client.lock_tokens(&guardian, &1);
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
    let (env, _, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, &token, 500);
    client.set_weight_threshold(&admin, &500);
    client.register_task(&admin, &1u64, &1u32);

    client.vote(&guardian, &1u64);
    assert_budget_limit!(env, COST_VOTE, "vote");
}

#[test]
fn test_gas_budget_vote_batch() {
    let (env, _, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, &token, 500);
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
    let guardian = add_guardian_with_rep(&env, &client, &admin, &token, 500);

    // Set a fee and treasury to hit the worst-case cross-contract call path
    let treasury = Address::generate(&env);
    client.set_treasury_address(&admin, &treasury);
    client.set_fee_bps(&admin, &1000u32); // 10% fee

    let sac = TestTokenClient::new(&env, &token);
    sac.mint(&guardian, &1000i128);

    client.lock_tokens(&guardian, &1000i128);
    assert_budget_limit!(env, COST_LOCK_TOKENS, "lock_tokens");
}

#[test]
fn test_gas_budget_cancel_task() {
    let (env, _, admin, _, client) = setup();

    client.register_task(&admin, &1u64, &1u32);
    client.cancel_task(&admin, &1u64);
    assert_budget_limit!(env, COST_CANCEL_TASK, "cancel_task");
}

#[test]
fn test_gas_budget_remove_guardian() {
    let (env, _, admin, _, client) = setup();
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    client.add_guardian(&admin, &first);
    client.add_guardian(&admin, &second);

    // Removing the first slot forces the dense-index swap-remove path
    // (slot != last_slot), matching the worst case the estimate models.
    client.remove_guardian(&admin, &first);
    assert_budget_limit!(env, COST_REMOVE_GUARDIAN, "remove_guardian");
}

#[test]
fn test_gas_budget_request_unlock() {
    let (env, _, _, _, client) = setup();
    let guardian = Address::generate(&env);

    client.request_unlock(&guardian);
    assert_budget_limit!(env, COST_REQUEST_UNLOCK, "request_unlock");
}

#[test]
fn test_gas_budget_set_vault_address() {
    let (env, _, admin, _, client) = setup();
    let vault = Address::generate(&env);

    client.set_vault_address(&admin, &vault);
    assert_budget_limit!(env, COST_SET_VAULT_ADDRESS, "set_vault_address");
}

#[test]
fn test_gas_budget_pause() {
    let (env, _, admin, _, client) = setup();
    client.grant_role(&admin, &admin, &Role::EmergencyManager);

    client.pause(&admin);
    assert_budget_limit!(env, COST_PAUSE, "pause");
}

#[test]
fn test_gas_budget_unpause() {
    let (env, _, admin, _, client) = setup();
    client.grant_role(&admin, &admin, &Role::EmergencyManager);
    client.pause(&admin);

    client.unpause(&admin);
    assert_budget_limit!(env, COST_UNPAUSE, "unpause");
}

#[test]
fn test_gas_budget_add_guardian() {
    let (env, _, admin, _, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    assert_budget_limit!(env, COST_ADD_GUARDIAN, "add_guardian");
}

#[test]
fn test_gas_budget_set_reputation() {
    let (env, _, admin, _, client) = setup();
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);

    client.set_reputation(&admin, &guardian, &500);
    assert_budget_limit!(env, COST_SET_REPUTATION, "set_reputation");
}

#[test]
fn test_gas_budget_set_weight_threshold() {
    let (env, _, admin, _, client) = setup();

    client.set_weight_threshold(&admin, &500);
    assert_budget_limit!(env, COST_SET_WEIGHT_THRESHOLD, "set_weight_threshold");
}

#[test]
fn test_gas_budget_toggle_pause() {
    let (env, _, admin, _, client) = setup();
    client.grant_role(&admin, &admin, &Role::EmergencyManager);

    client.toggle_pause(&admin);
    assert_budget_limit!(env, COST_TOGGLE_PAUSE, "toggle_pause");
}

#[test]
fn test_batch_execute_rejects_batch_over_budget() {
    let (env, _, admin, _, client) = setup();

    // 80 × COST_REGISTER_TASK (1.3M) = 104M, over MAX_BATCH_EXECUTE_COST (100M),
    // so the pre-flight cost sum must reject before any task is registered.
    let mut calls = Vec::new(&env);
    for task_id in 1..=80u64 {
        calls.push_back(BatchCall::RegisterTask(admin.clone(), task_id, 1u32));
    }

    let result = client.try_batch_execute(&calls);
    assert!(matches!(result, Err(Ok(ContractError::BatchTooLarge))));

    // Rejected before dispatch: no task should have been registered.
    assert!(client.get_task(&1u64).is_none());
    assert!(client.get_task(&80u64).is_none());
}
