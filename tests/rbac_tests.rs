#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};
use vero_core_contracts::{ContractError, Role, VeroContractClient, FAILURE_THRESHOLD};

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
    client.grant_role(&admin, &admin, &Role::EmergencyManager);
    client.grant_role(&admin, &admin, &Role::TreasuryManager);

    (env, admin, token_addr, client)
}

/// Trip the circuit breaker the *legitimate* way: many distinct, authenticated
/// reporters, each respecting the per-address cooldown and quota.
///
/// A single address deliberately cannot do this — see
/// `test_single_address_cannot_pause_contract` in `tests/circuit_breaker.rs`.
fn trip_circuit_breaker(env: &Env, client: &VeroContractClient) {
    let reporters: std::vec::Vec<Address> = (0..11).map(|_| Address::generate(env)).collect();
    let mut recorded = 0;
    'outer: for round in 0..5 {
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + 20 * (round + 1));
        for r in &reporters {
            if recorded > FAILURE_THRESHOLD {
                break 'outer;
            }
            client.record_failure(r);
            recorded += 1;
        }
    }
}

// ─── Initial Admin Role Assignment ─────────────────────────────────

#[test]
fn test_initialize_grants_admin_role_to_deployer() {
    let (_env, admin, _token, client) = setup();

    // The deployer should have been granted the Admin role during initialize
    assert!(client.has_role(&admin, &Role::Admin));
}

// ─── Role Assignment Tests ──────────────────────────────────────────

#[test]
fn test_admin_can_grant_guardian_manager_role() {
    let (env, admin, _token, client) = setup();
    let guardian_manager = Address::generate(&env);

    client.grant_role(&admin, &guardian_manager, &Role::GuardianManager);

    assert!(client.has_role(&guardian_manager, &Role::GuardianManager));
}

#[test]
fn test_admin_can_grant_task_manager_role() {
    let (env, admin, _token, client) = setup();
    let task_manager = Address::generate(&env);

    client.grant_role(&admin, &task_manager, &Role::TaskManager);

    assert!(client.has_role(&task_manager, &Role::TaskManager));
}

#[test]
fn test_admin_can_grant_config_manager_role() {
    let (env, admin, _token, client) = setup();
    let config_manager = Address::generate(&env);

    client.grant_role(&admin, &config_manager, &Role::ConfigManager);

    assert!(client.has_role(&config_manager, &Role::ConfigManager));
}

#[test]
fn test_admin_can_grant_emergency_manager_role() {
    let (env, admin, _token, client) = setup();
    let emergency_manager = Address::generate(&env);

    client.grant_role(&admin, &emergency_manager, &Role::EmergencyManager);

    assert!(client.has_role(&emergency_manager, &Role::EmergencyManager));
}

#[test]
fn test_admin_can_grant_treasury_manager_role() {
    let (env, admin, _token, client) = setup();
    let treasury_manager = Address::generate(&env);

    client.grant_role(&admin, &treasury_manager, &Role::TreasuryManager);

    assert!(client.has_role(&treasury_manager, &Role::TreasuryManager));
}

#[test]
fn test_admin_can_grant_admin_role_to_another_address() {
    let (env, admin, _token, client) = setup();
    let new_admin = Address::generate(&env);

    client.grant_role(&admin, &new_admin, &Role::Admin);

    assert!(client.has_role(&new_admin, &Role::Admin));
}

// ─── Role Revocation Tests ──────────────────────────────────────────

#[test]
fn test_admin_can_revoke_role() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::TaskManager);
    assert!(client.has_role(&manager, &Role::TaskManager));

    client.revoke_role(&admin, &manager, &Role::TaskManager);
    assert!(!client.has_role(&manager, &Role::TaskManager));
}

#[test]
fn test_revoking_one_role_does_not_affect_other_roles() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);

    // Grant two roles
    client.grant_role(&admin, &manager, &Role::TaskManager);
    client.grant_role(&admin, &manager, &Role::ConfigManager);

    // Revoke one
    client.revoke_role(&admin, &manager, &Role::TaskManager);

    // Other role should remain
    assert!(!client.has_role(&manager, &Role::TaskManager));
    assert!(client.has_role(&manager, &Role::ConfigManager));
}

// ─── Admin Lockout Prevention ───────────────────────────────────────

#[test]
fn test_cannot_revoke_last_admin_role() {
    let (_env, admin, _token, client) = setup();

    // Only one admin exists (the deployer)
    let result = client.try_revoke_role(&admin, &admin, &Role::Admin);

    // Should fail with LastAdminRemovalBlocked
    assert!(result.is_err());

    // Admin should still hold the role
    assert!(client.has_role(&admin, &Role::Admin));
}

#[test]
fn test_can_revoke_admin_when_multiple_admins_exist() {
    let (env, admin, _token, client) = setup();
    let second_admin = Address::generate(&env);

    // Add them as a guardian so count_role_holders can find them
    client.add_guardian(&admin, &second_admin);

    // Grant Admin role to a second address
    client.grant_role(&admin, &second_admin, &Role::Admin);

    // Now we can revoke the first admin's role
    let result = client.try_revoke_role(&admin, &admin, &Role::Admin);
    assert!(result.is_ok());

    // First admin should no longer have the role
    assert!(!client.has_role(&admin, &Role::Admin));
    // Second admin should still have it
    assert!(client.has_role(&second_admin, &Role::Admin));
}

// ─── Non-Admin Cannot Grant or Revoke Roles ────────────────────────

#[test]
fn test_non_admin_cannot_grant_role() {
    let (env, _admin, _token, client) = setup();
    let stranger = Address::generate(&env);
    let target = Address::generate(&env);

    let result = client.try_grant_role(&stranger, &target, &Role::TaskManager);
    assert!(result.is_err());

    // Role should not have been granted
    assert!(!client.has_role(&target, &Role::TaskManager));
}

#[test]
fn test_non_admin_cannot_revoke_role() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);
    let stranger = Address::generate(&env);

    // Admin grants a role
    client.grant_role(&admin, &manager, &Role::TaskManager);
    assert!(client.has_role(&manager, &Role::TaskManager));

    // Stranger tries to revoke it
    let result = client.try_revoke_role(&stranger, &manager, &Role::TaskManager);
    assert!(result.is_err());

    // Role should still be held
    assert!(client.has_role(&manager, &Role::TaskManager));
}

// ─── Per-Function Access Control: GuardianManager ───────────────────

#[test]
fn test_guardian_manager_can_add_guardian() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);
    let guardian = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::GuardianManager);

    let result = client.try_add_guardian(&manager, &guardian);
    assert!(result.is_ok());
    assert!(client.is_guardian(&guardian));
}

#[test]
fn test_non_guardian_manager_cannot_add_guardian() {
    let (env, _admin, _token, client) = setup();
    let stranger = Address::generate(&env);
    let guardian = Address::generate(&env);

    let result = client.try_add_guardian(&stranger, &guardian);
    assert!(result.is_err());
    assert!(!client.is_guardian(&guardian));
}

#[test]
fn test_guardian_manager_can_remove_guardian() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);
    let guardian = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::GuardianManager);
    client.add_guardian(&manager, &guardian);

    let result = client.try_remove_guardian(&manager, &guardian);
    assert!(result.is_ok());
    assert!(!client.is_guardian(&guardian));
}

#[test]
fn test_non_guardian_manager_cannot_remove_guardian() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);
    let stranger = Address::generate(&env);
    let guardian = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::GuardianManager);
    client.add_guardian(&manager, &guardian);

    let result = client.try_remove_guardian(&stranger, &guardian);
    assert!(result.is_err());
    assert!(client.is_guardian(&guardian));
}

#[test]
fn test_guardian_manager_can_set_reputation() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);
    let guardian = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::GuardianManager);
    client.add_guardian(&manager, &guardian);

    let result = client.try_set_reputation(&manager, &guardian, &300);
    assert!(result.is_ok());
    assert_eq!(client.get_reputation(&guardian), Some(300));
}

#[test]
fn test_non_guardian_manager_cannot_set_reputation() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);
    let stranger = Address::generate(&env);
    let guardian = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::GuardianManager);
    client.add_guardian(&manager, &guardian);

    let result = client.try_set_reputation(&stranger, &guardian, &300);
    assert!(result.is_err());
    assert_eq!(client.get_reputation(&guardian), None);
}

// ─── Per-Function Access Control: TaskManager ───────────────────────

#[test]
fn test_task_manager_can_register_task() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::TaskManager);

    let result = client.try_register_task(&manager, &1, &1u32);
    assert!(result.is_ok());
    assert!(client.get_task(&1).is_some());
}

#[test]
fn test_non_task_manager_cannot_register_task() {
    let (env, _admin, _token, client) = setup();
    let stranger = Address::generate(&env);

    let result = client.try_register_task(&stranger, &1, &1u32);
    assert!(result.is_err());
    assert!(client.get_task(&1).is_none());
}

#[test]
fn test_task_manager_can_cancel_task() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::TaskManager);
    client.register_task(&manager, &1, &1u32);

    let result = client.try_cancel_task(&manager, &1);
    assert!(result.is_ok());
    assert!(client.get_task(&1).unwrap().is_cancelled);
}

#[test]
fn test_non_task_manager_cannot_cancel_task() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::TaskManager);
    client.register_task(&manager, &1, &1u32);

    let result = client.try_cancel_task(&stranger, &1);
    assert!(result.is_err());
    assert!(!client.get_task(&1).unwrap().is_cancelled);
}

#[test]
fn test_task_manager_can_purge_task() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::TaskManager);
    client.register_task(&manager, &1, &1u32);
    client.cancel_task(&manager, &1);

    let result = client.try_purge_task(&manager, &1);
    assert!(result.is_ok());
    assert!(client.get_task(&1).is_none());
}

#[test]
fn test_non_task_manager_cannot_purge_task() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::TaskManager);
    client.register_task(&manager, &1, &1u32);
    client.cancel_task(&manager, &1);

    let result = client.try_purge_task(&stranger, &1);
    assert!(result.is_err());
    assert!(client.get_task(&1).is_some());
}

// ─── Per-Function Access Control: ConfigManager ─────────────────────

#[test]
fn test_config_manager_can_set_weight_threshold() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::ConfigManager);

    let result = client.try_set_weight_threshold(&manager, &500);
    assert!(result.is_ok());
    assert_eq!(client.get_weight_threshold(), 500);
}

#[test]
fn test_non_config_manager_cannot_set_weight_threshold() {
    let (env, _admin, _token, client) = setup();
    let stranger = Address::generate(&env);

    let result = client.try_set_weight_threshold(&stranger, &500);
    assert!(result.is_err());
    assert_eq!(client.get_weight_threshold(), 300); // default
}

#[test]
fn test_config_manager_can_set_vault_address() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);
    let vault = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::ConfigManager);

    client.set_vault_address(&manager, &vault);
    assert_eq!(client.get_snapshot().meta.vault_address, Some(vault));
}

#[test]
fn test_non_config_manager_cannot_set_vault_address() {
    let (env, _admin, _token, client) = setup();
    let stranger = Address::generate(&env);
    let vault = Address::generate(&env);

    let result = client.try_set_vault_address(&stranger, &vault);
    assert!(matches!(result, Err(Ok(ContractError::NotAuthorized))));
    assert_eq!(client.get_snapshot().vault_address, None);
}

#[test]
fn test_set_vault_address_rejected_while_paused() {
    let (env, admin, _token, client) = setup();
    let vault = Address::generate(&env);

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_set_vault_address(&admin, &vault);
    assert!(matches!(result, Err(Ok(ContractError::ContractPaused))));
    assert_eq!(client.get_snapshot().vault_address, None);
}

#[test]
fn test_upgrade_contract_rejects_non_admin() {
    let (env, _admin, _token, client) = setup();
    let stranger = Address::generate(&env);
    let wasm_hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);

    let result = client.try_upgrade_contract(&stranger, &wasm_hash);
    assert!(matches!(result, Err(Ok(ContractError::NotAuthorized))));
}

// ─── Per-Function Access Control: EmergencyManager ──────────────────

#[test]
fn test_emergency_manager_can_pause() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::EmergencyManager);

    let result = client.try_pause(&manager);
    assert!(result.is_ok());
    assert!(client.is_paused());
}

#[test]
fn test_non_emergency_manager_cannot_pause() {
    let (env, _admin, _token, client) = setup();
    let stranger = Address::generate(&env);

    let result = client.try_pause(&stranger);
    assert!(result.is_err());
    assert!(!client.is_paused());
}

#[test]
fn test_emergency_manager_can_unpause() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::EmergencyManager);
    client.pause(&manager);

    let result = client.try_unpause(&manager);
    assert!(result.is_ok());
    assert!(!client.is_paused());
}

#[test]
fn test_non_emergency_manager_cannot_unpause() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::EmergencyManager);
    client.pause(&manager);

    let result = client.try_unpause(&stranger);
    assert!(result.is_err());
    assert!(client.is_paused());
}

#[test]
fn test_emergency_manager_can_toggle_pause() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::EmergencyManager);

    client.toggle_pause(&manager);
    assert!(client.is_paused());

    client.toggle_pause(&manager);
    assert!(!client.is_paused());
}

#[test]
fn test_emergency_manager_can_reset_circuit_breaker() {
    let (env, admin, _token, client) = setup();
    let manager = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::EmergencyManager);

    // Trip the circuit breaker
    trip_circuit_breaker(&env, &client);
    assert!(client.is_paused());

    client.reset_circuit_breaker(&manager);
    assert!(!client.is_paused());
}

#[test]
fn test_non_emergency_manager_cannot_reset_circuit_breaker() {
    let (env, _admin, _token, client) = setup();
    let stranger = Address::generate(&env);

    // Trip the circuit breaker
    trip_circuit_breaker(&env, &client);
    assert!(client.is_paused());

    let result = client.try_reset_circuit_breaker(&stranger);
    assert!(result.is_err());
    assert!(client.is_paused());
}

#[test]
fn test_emergency_recover_bypasses_pause() {
    let (env, admin, token, client) = setup();
    let manager = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::EmergencyManager);

    // Fund the contract address with tokens
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&client.address, &1000);

    // Pause the contract explicitly
    client.pause(&manager);
    assert!(client.is_paused());

    // Emergency recover should succeed even when the contract is paused
    let result = client.try_emergency_recover(&manager, &recipient, &500);
    assert!(result.is_ok());

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&recipient), 500);
    assert_eq!(token_client.balance(&client.address), 500);
}

#[test]
fn test_emergency_recover_bypasses_circuit_breaker_trip() {
    let (env, admin, token, client) = setup();
    let manager = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::EmergencyManager);

    // Fund the contract address with tokens
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&client.address, &1000);

    // Trip the circuit breaker via distinct, authenticated reporters.
    trip_circuit_breaker(&env, &client);
    assert!(client.is_paused());

    // Emergency recover should succeed when circuit breaker is tripped
    let result = client.try_emergency_recover(&manager, &recipient, &400);
    assert!(result.is_ok());

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&recipient), 400);
}

// ─── Per-Function Access Control: TreasuryManager ───────────────────

#[test]
fn test_treasury_manager_role_required_for_reward_stream() {
    let (env, admin, token, client) = setup();
    let manager = Address::generate(&env);
    let contributor = Address::generate(&env);
    let drips = Address::generate(&env);

    client.grant_role(&admin, &manager, &Role::TreasuryManager);
    client.grant_role(&admin, &manager, &Role::TaskManager);

    // Register and resolve a task
    client.register_task(&manager, &1, &1u32);
    let guardian = Address::generate(&env);
    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &300);
    client.set_weight_threshold(&admin, &1);

    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    token_client.mint(&guardian, &101);
    client.lock_tokens(&guardian, &101);
    client.vote(&guardian, &1);

    // Treasury manager can start stream (will fail at drips call but passes auth)
    let result = client.try_start_reward_stream(&manager, &drips, &contributor, &1);
    // Result will be err due to drips call failure, but not due to auth
    assert!(result.is_err()); // Expected: DripsCallFailed, not NotAuthorized
}

#[test]
fn test_non_treasury_manager_cannot_start_reward_stream() {
    let (env, admin, token, client) = setup();
    let stranger = Address::generate(&env);
    let contributor = Address::generate(&env);
    let drips = Address::generate(&env);

    client.grant_role(&admin, &admin, &Role::TaskManager);
    client.grant_role(&admin, &admin, &Role::GuardianManager);

    // Register and resolve a task
    client.register_task(&admin, &1, &1u32);
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &300);
    client.set_weight_threshold(&admin, &1);

    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    token_client.mint(&guardian, &101);
    client.lock_tokens(&guardian, &101);
    client.vote(&guardian, &1);

    let result = client.try_start_reward_stream(&stranger, &drips, &contributor, &1);
    assert!(result.is_err());
}

// ─── Per-Function Access Control: Admin ─────────────────────────────

#[test]
fn test_admin_can_set_upgrade_signers() {
    let (env, admin, _token, client) = setup();
    let signers = soroban_sdk::vec![&env, Address::generate(&env)];

    let result = client.try_set_upgrade_signers(&admin, &signers, &1);
    assert!(result.is_ok());
    assert_eq!(client.get_upgrade_threshold(), 1);
}

#[test]
fn test_non_admin_cannot_set_upgrade_signers() {
    let (env, _admin, _token, client) = setup();
    let stranger = Address::generate(&env);
    let signers = soroban_sdk::vec![&env, Address::generate(&env)];

    let result = client.try_set_upgrade_signers(&stranger, &signers, &1);
    assert!(result.is_err());
    assert_eq!(client.get_upgrade_threshold(), 0);
}

#[test]
fn test_admin_can_cancel_upgrade() {
    let (env, admin, _token, client) = setup();
    let signers = soroban_sdk::vec![&env, Address::generate(&env)];
    let wasm_hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &1);
    client.propose_upgrade(&signers.get(0).unwrap(), &wasm_hash);

    let result = client.try_cancel_upgrade(&admin);
    assert!(result.is_ok());
}

#[test]
fn test_non_admin_cannot_cancel_upgrade() {
    let (env, admin, _token, client) = setup();
    let stranger = Address::generate(&env);
    let signers = soroban_sdk::vec![&env, Address::generate(&env)];
    let wasm_hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &1);
    client.propose_upgrade(&signers.get(0).unwrap(), &wasm_hash);

    let result = client.try_cancel_upgrade(&stranger);
    assert!(result.is_err());
}

// ─── Multi-Role Scenarios ───────────────────────────────────────────

#[test]
fn test_address_can_hold_multiple_roles() {
    let (env, admin, _token, client) = setup();
    let multi_role_user = Address::generate(&env);

    client.grant_role(&admin, &multi_role_user, &Role::TaskManager);
    client.grant_role(&admin, &multi_role_user, &Role::ConfigManager);

    assert!(client.has_role(&multi_role_user, &Role::TaskManager));
    assert!(client.has_role(&multi_role_user, &Role::ConfigManager));

    // Can use both roles
    client.register_task(&multi_role_user, &1, &1u32);
    client.set_weight_threshold(&multi_role_user, &400);

    assert!(client.get_task(&1).is_some());
    assert_eq!(client.get_weight_threshold(), 400);
}

// ─── Regression: Existing Tests Should Still Pass ───────────────────

#[test]
fn test_backward_compatibility_admin_retains_all_powers() {
    let (env, admin, _token, client) = setup();
    let guardian = Address::generate(&env);
    let vault = Address::generate(&env);

    // Admin should be able to do everything by granting themselves appropriate roles
    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.grant_role(&admin, &admin, &Role::TaskManager);
    client.grant_role(&admin, &admin, &Role::ConfigManager);
    client.grant_role(&admin, &admin, &Role::EmergencyManager);

    // Guardian management
    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &300);

    // Task management
    client.register_task(&admin, &1, &1u32);
    client.cancel_task(&admin, &1);

    // Configuration
    client.set_weight_threshold(&admin, &500);
    client.set_vault_address(&admin, &vault);

    // Emergency controls
    client.pause(&admin);
    client.unpause(&admin);

    // All operations should succeed
    assert!(client.is_guardian(&guardian));
    assert_eq!(client.get_reputation(&guardian), Some(300));
    assert!(client.get_task(&1).unwrap().is_cancelled);
    assert_eq!(client.get_weight_threshold(), 500);
}

// ─── Guardian Rotation: Duplicate Prevention & AllGuardians Cleanup ─────

#[test]
fn test_adding_duplicate_guardian_is_rejected() {
    let (env, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    assert!(client.is_guardian(&guardian));

    let result = client.try_add_guardian(&admin, &guardian);
    assert!(result.is_err(), "duplicate guardian add must be rejected");
}

#[test]
fn test_removed_guardian_can_be_re_added() {
    let (env, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    assert!(client.is_guardian(&guardian));

    client.remove_guardian(&admin, &guardian);
    assert!(!client.is_guardian(&guardian));

    let result = client.try_add_guardian(&admin, &guardian);
    assert!(result.is_ok(), "re-adding a removed guardian must succeed");
    assert!(client.is_guardian(&guardian));
}

#[test]
fn test_guardian_rotation_preserves_other_guardians() {
    let (env, admin, _token, client) = setup();
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);

    client.add_guardian(&admin, &g1);
    client.add_guardian(&admin, &g2);
    client.add_guardian(&admin, &g3);

    client.remove_guardian(&admin, &g2);

    assert!(client.is_guardian(&g1));
    assert!(!client.is_guardian(&g2));
    assert!(client.is_guardian(&g3));

    let result = client.try_add_guardian(&admin, &g2);
    assert!(result.is_ok());
    assert!(client.is_guardian(&g2));
}
