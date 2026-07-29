#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, String};
use vero_core_contracts::{
    ContractError, Role, VeroContractClient, ZERO_ADDRESS_STR,
};

fn zero_address(env: &Env) -> Address {
    Address::from_string(&String::from_str(env, ZERO_ADDRESS_STR))
}

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
    client.grant_role(&admin, &admin, &Role::EmergencyManager);
    client.grant_role(&admin, &admin, &Role::TreasuryManager);

    (env, admin, token_addr, client)
}

// ─── Role assignment / revocation ───────────────────────────────────

#[test]
fn test_grant_role_rejects_zero_target() {
    let (env, admin, _token, client) = setup();
    let zero = zero_address(&env);

    let result = client.try_grant_role(&admin, &zero, &Role::TaskManager);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
    assert!(!client.has_role(&zero, &Role::TaskManager));
}

#[test]
fn test_revoke_role_rejects_zero_target() {
    let (env, admin, _token, client) = setup();
    let zero = zero_address(&env);

    let result = client.try_revoke_role(&admin, &zero, &Role::TaskManager);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
}

#[test]
fn test_grant_role_rejects_zero_caller() {
    let (env, _admin, _token, client) = setup();
    let zero = zero_address(&env);
    let target = Address::generate(&env);

    let result = client.try_grant_role(&zero, &target, &Role::TaskManager);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
    assert!(!client.has_role(&target, &Role::TaskManager));
}

#[test]
fn test_legitimate_role_assignment_still_works() {
    let (env, admin, _token, client) = setup();
    let target = Address::generate(&env);

    client.grant_role(&admin, &target, &Role::TaskManager);
    assert!(client.has_role(&target, &Role::TaskManager));
}

// ─── Guardian management ────────────────────────────────────────────

#[test]
fn test_add_guardian_rejects_zero_guardian() {
    let (env, admin, _token, client) = setup();
    let zero = zero_address(&env);

    let result = client.try_add_guardian(&admin, &zero);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
    assert!(!client.is_guardian(&zero));
}

#[test]
fn test_remove_guardian_rejects_zero_guardian() {
    let (env, admin, _token, client) = setup();
    let zero = zero_address(&env);

    let result = client.try_remove_guardian(&admin, &zero);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
}

#[test]
fn test_set_reputation_rejects_zero_guardian() {
    let (env, admin, _token, client) = setup();
    let zero = zero_address(&env);

    let result = client.try_set_reputation(&admin, &zero, &300);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
}

// ─── Treasury / config ──────────────────────────────────────────────

#[test]
fn test_set_treasury_address_rejects_zero_treasury() {
    let (env, admin, _token, client) = setup();
    let zero = zero_address(&env);

    let result = client.try_set_treasury_address(&admin, &zero);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
}

#[test]
fn test_start_reward_stream_rejects_zero_contributor() {
    let (env, admin, _token, client) = setup();
    let zero = zero_address(&env);
    let drips = Address::generate(&env);

    let result = client.try_start_reward_stream(&admin, &drips, &zero, &1);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
}

#[test]
fn test_set_fee_bps_rejects_zero_admin() {
    let (env, _admin, _token, client) = setup();
    let zero = zero_address(&env);

    let result = client.try_set_fee_bps(&zero, &100);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
}

// ─── Task management ────────────────────────────────────────────────

#[test]
fn test_register_task_rejects_zero_admin() {
    let (env, _admin, _token, client) = setup();
    let zero = zero_address(&env);

    let result = client.try_register_task(&zero, &1, &1u32);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
    assert!(client.get_task(&1).is_none());
}

#[test]
fn test_cancel_task_rejects_zero_admin() {
    let (env, admin, _token, client) = setup();
    let zero = zero_address(&env);
    client.register_task(&admin, &1, &1u32);

    let result = client.try_cancel_task(&zero, &1);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
    assert!(!client.get_task(&1).unwrap().is_cancelled);
}

// ─── Voting / locking ───────────────────────────────────────────────

#[test]
fn test_vote_rejects_zero_guardian() {
    let (env, admin, _token, client) = setup();
    let zero = zero_address(&env);
    client.register_task(&admin, &1, &1u32);

    let result = client.try_vote(&zero, &1);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
}

#[test]
fn test_lock_tokens_rejects_zero_guardian() {
    let (env, _admin, _token, client) = setup();
    let zero = zero_address(&env);

    let result = client.try_lock_tokens(&zero, &100);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
}

// ─── Upgrade signers ────────────────────────────────────────────────

#[test]
fn test_set_upgrade_signers_rejects_zero_signer() {
    let (env, admin, _token, client) = setup();
    let zero = zero_address(&env);
    let valid = Address::generate(&env);
    let signers = soroban_sdk::vec![&env, valid, zero];

    let result = client.try_set_upgrade_signers(&admin, &signers, &2);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
}

#[test]
fn test_propose_upgrade_rejects_zero_signer() {
    let (env, _admin, _token, client) = setup();
    let zero = zero_address(&env);
    let hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);

    let result = client.try_propose_upgrade(&zero, &hash);
    assert!(matches!(result, Err(Ok(ContractError::InvalidAddress))));
}
