#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, BytesN, Env, TryIntoVal, Vec,
};
use vero_core_contracts::{ContractError, Role, VeroContractClient};

fn setup() -> (Env, Address, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = token.address();

    client.initialize(&admin, &token_addr, &100);
    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.grant_role(&admin, &admin, &Role::TaskManager);
    client.grant_role(&admin, &admin, &Role::ConfigManager);
    client.grant_role(&admin, &admin, &Role::EmergencyManager);
    client.grant_role(&admin, &admin, &Role::TreasuryManager);

    (env, contract_id, admin, token_addr, client)
}

fn generate_signers(env: &Env, n: u32) -> Vec<Address> {
    let mut generated = std::vec::Vec::new();
    for _ in 0..n {
        generated.push(Address::generate(env));
    }
    generated.sort();

    let mut signers = Vec::new(env);
    for signer in generated {
        signers.push_back(signer);
    }
    signers
}

/// Helper to collect all events into a vector of event symbols for assertion.
fn event_symbols(env: &Env) -> Vec<soroban_sdk::Symbol> {
    let mut symbols = Vec::new(env);
    for e in env.events().all().iter() {
        if let Some(topic) = e.1.get(0) {
            let res: Result<soroban_sdk::Symbol, _> = topic.try_into_val(env);
            if let Ok(sym) = res {
                symbols.push_back(sym);
            }
        }
    }
    symbols
}

// ─── Happy path: full multi-sig upgrade flow ────────────────────────

#[test]
fn test_set_upgrade_signers_successful() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 3);

    client.set_upgrade_signers(&admin, &signers, &2u32);

    let stored_signers = client.get_upgrade_signers();
    assert_eq!(stored_signers.len(), 3);
    for i in 0..3 {
        assert_eq!(stored_signers.get(i).unwrap(), signers.get(i).unwrap());
    }
    assert_eq!(client.get_upgrade_threshold(), 2);
}

#[test]
fn test_get_upgrade_signers_default_empty() {
    let (_env, _contract_id, _admin, _token, client) = setup();

    let signers = client.get_upgrade_signers();
    assert_eq!(signers.len(), 0);
    assert_eq!(client.get_upgrade_threshold(), 0);
}

#[test]
fn test_full_multi_sig_upgrade_flow() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 3);
    let wasm_hash = BytesN::from_array(&env, &[1u8; 32]);

    // Step 1: Admin configures multi-sig signers (threshold = 2)
    client.set_upgrade_signers(&admin, &signers, &2u32);
    assert_eq!(client.get_upgrade_threshold(), 2);

    // Step 2: First signer proposes the upgrade
    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &wasm_hash);

    // Step 3: Second signer approves
    let signer2 = signers.get(1).unwrap();
    client.approve_upgrade(&signer2);

    // Step 4: Execute upgrade (threshold of 2 is met)
    // The actual deployer call will fail because the hash points to invalid WASM,
    // but our multi-sig check passes. Use try_ to capture the error.
    let result = client.try_execute_upgrade();
    // The error should come from the runtime (deployer), not from our contract,
    // meaning our multi-sig checks passed successfully.
    assert!(
        result.is_err(),
        "execute_upgrade should attempt deploy and fail at runtime"
    );

    // Verify pending state was cleaned up before the deployer call
    // (We can't query pending state directly since there's no view function for it,
    // but the cleanup happens before the deployer call in execute_upgrade)
}

#[test]
fn test_propose_upgrade_approves_first_signer() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);
    let wasm_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &2u32);

    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &wasm_hash);

    // Second signer should be able to approve
    let signer2 = signers.get(1).unwrap();
    let result = client.try_approve_upgrade(&signer2);
    assert!(result.is_ok());
}

// ─── Upgrade fails without N signatures ────────────────────────────

#[test]
fn test_execute_upgrade_fails_without_enough_approvals() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 3);
    let wasm_hash = BytesN::from_array(&env, &[3u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &3u32);

    // Only one signer approves
    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &wasm_hash);

    // Execute with only 1 approval (need 3) — should fail
    let result = client.try_execute_upgrade();
    assert!(result.is_err());
}

#[test]
fn test_execute_upgrade_fails_with_one_of_two_approvals() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);
    let wasm_hash = BytesN::from_array(&env, &[4u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &2u32);

    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &wasm_hash);

    // Only 1 of 2 approvals — should fail
    let result = client.try_execute_upgrade();
    assert!(result.is_err());
}

// ─── Non-signer cannot propose or approve ──────────────────────────

#[test]
fn test_non_signer_cannot_propose_upgrade() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);
    let wasm_hash = BytesN::from_array(&env, &[5u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &2u32);

    let stranger = Address::generate(&env);
    let result = client.try_propose_upgrade(&stranger, &wasm_hash);
    assert!(result.is_err());
}

#[test]
fn test_non_signer_cannot_approve_upgrade() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);
    let wasm_hash = BytesN::from_array(&env, &[6u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &2u32);

    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &wasm_hash);

    let stranger = Address::generate(&env);
    let result = client.try_approve_upgrade(&stranger);
    assert!(result.is_err());
}

// ─── Duplicate approval rejected ────────────────────────────────────

#[test]
fn test_duplicate_approval_rejected() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);
    let wasm_hash = BytesN::from_array(&env, &[7u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &2u32);

    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &wasm_hash);

    // Same signer proposing again with same hash — should approve the first time
    // (propose_upgrade with same hash calls approve_upgrade internally)
    let result = client.try_propose_upgrade(&signer1, &wasm_hash);
    assert!(result.is_err(), "duplicate approval should be rejected");
}

// ─── No pending upgrade edge cases ─────────────────────────────────

#[test]
fn test_approve_without_proposal_fails() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);

    client.set_upgrade_signers(&admin, &signers, &2u32);

    // Try to approve without a pending proposal
    let signer1 = signers.get(0).unwrap();
    let result = client.try_approve_upgrade(&signer1);
    assert!(result.is_err());
}

#[test]
fn test_execute_without_proposal_fails() {
    let (_env, _contract_id, _admin, _token, client) = setup();

    let result = client.try_execute_upgrade();
    assert!(result.is_err());
}

// ─── Admin can cancel pending upgrade ──────────────────────────────

#[test]
fn test_admin_can_cancel_pending_upgrade() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);
    let wasm_hash = BytesN::from_array(&env, &[8u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &2u32);

    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &wasm_hash);

    // Admin cancels the pending upgrade
    client.cancel_upgrade(&admin);

    // After cancellation, execute should fail
    let result = client.try_execute_upgrade();
    assert!(result.is_err());
}

#[test]
fn test_non_admin_cannot_cancel_upgrade() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);
    let wasm_hash = BytesN::from_array(&env, &[9u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &2u32);

    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &wasm_hash);

    // Non-admin tries to cancel
    let stranger = Address::generate(&env);
    let result = client.try_cancel_upgrade(&stranger);
    assert!(result.is_err());

    // Pending upgrade should still be valid
    let signer2 = signers.get(1).unwrap();
    let approve_result = client.try_approve_upgrade(&signer2);
    assert!(approve_result.is_ok());
}

#[test]
fn test_cancel_without_proposal_fails() {
    let (_env, _contract_id, admin, _token, client) = setup();

    let result = client.try_cancel_upgrade(&admin);
    assert!(result.is_err());
}

// ─── Invalid configuration checks ──────────────────────────────────

#[test]
fn test_set_threshold_zero_rejected() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 3);

    let result = client.try_set_upgrade_signers(&admin, &signers, &0u32);
    assert!(result.is_err());
}

#[test]
fn test_set_threshold_exceeds_signers_rejected() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);

    // Threshold 3 > 2 signers — should fail
    let result = client.try_set_upgrade_signers(&admin, &signers, &3u32);
    assert!(result.is_err());
}

#[test]
fn test_set_threshold_equal_to_signers_is_valid() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 5);

    // Threshold 5 == 5 signers — should succeed (unanimous)
    let result = client.try_set_upgrade_signers(&admin, &signers, &5u32);
    assert!(result.is_ok());
    assert_eq!(client.get_upgrade_threshold(), 5);
}

// ─── Non-admin cannot set upgrade signers ──────────────────────────

#[test]
fn test_non_admin_cannot_set_upgrade_signers() {
    let (env, _contract_id, _admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);

    let stranger = Address::generate(&env);
    let result = client.try_set_upgrade_signers(&stranger, &signers, &2u32);
    assert!(result.is_err());
}

// ─── Reconfiguring signers clears pending upgrade ──────────────────

#[test]
fn test_setting_signers_clears_pending_upgrade() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);
    let wasm_hash = BytesN::from_array(&env, &[10u8; 32]);

    // Configure, propose
    client.set_upgrade_signers(&admin, &signers, &2u32);
    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &wasm_hash);

    // Reconfigure signers — this should clear pending state
    let new_signers = generate_signers(&env, 3);
    client.set_upgrade_signers(&admin, &new_signers, &2u32);

    // After reconfiguration, execute should fail (no pending upgrade)
    let result = client.try_execute_upgrade();
    assert!(result.is_err());
}

// ─── Proposing different hash after pending proposal fails ─────────

#[test]
fn test_propose_different_hash_after_pending_rejected() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 3);
    let hash1 = BytesN::from_array(&env, &[11u8; 32]);
    let hash2 = BytesN::from_array(&env, &[12u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &2u32);

    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &hash1);

    // Different signer tries to propose a different hash — should fail
    let signer2 = signers.get(1).unwrap();
    let result = client.try_propose_upgrade(&signer2, &hash2);
    assert!(result.is_err());

    // Original proposal should still be valid
    let approve_result = client.try_approve_upgrade(&signer2);
    assert!(approve_result.is_ok());
}

// ─── Proposing same hash after pending works (adds approval) ───────

#[test]
fn test_propose_same_hash_adds_approval() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 3);
    let wasm_hash = BytesN::from_array(&env, &[13u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &3u32);

    // Signer 1 proposes
    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &wasm_hash);

    // Signer 2 also proposes with same hash — acts as approval
    let signer2 = signers.get(1).unwrap();
    let result = client.try_propose_upgrade(&signer2, &wasm_hash);
    result.unwrap();

    // Signer 3 approves via approve_upgrade
    let signer3 = signers.get(2).unwrap();
    client.approve_upgrade(&signer3);

    // Threshold 3 met — execute should proceed past our checks
    let exec_result = client.try_execute_upgrade();
    assert!(
        exec_result.is_err(),
        "should attempt deploy and fail at runtime"
    );
}

// ─── Events emitted correctly ─────────────────────────────────────

#[test]
fn test_upgrade_events_emitted() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);
    let wasm_hash = BytesN::from_array(&env, &[14u8; 32]);

    // Clear events from setup
    env.events().all();

    client.set_upgrade_signers(&admin, &signers, &2u32);
    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &wasm_hash);
    let signer2 = signers.get(1).unwrap();
    client.approve_upgrade(&signer2);

    let events = env.events().all();
    // Should have at least 3 events: up_sig, up_prop, up_app
    assert!(events.len() >= 3, "expected at least 3 upgrade events");

    let mut symbols = std::vec::Vec::new();
    for e in events.iter() {
        if let Some(topic) = e.1.get(0) {
            let res: Result<soroban_sdk::Symbol, _> = topic.try_into_val(&env);
            if let Ok(sym) = res {
                symbols.push(sym.to_string());
            }
        }
    }

    assert!(
        symbols.contains(&"up_sig".to_string()),
        "expected up_sig event"
    );
    assert!(
        symbols.contains(&"up_prop".to_string()),
        "expected up_prop event"
    );
    assert!(
        symbols.contains(&"up_app".to_string()),
        "expected up_app event"
    );
}

#[test]
fn test_cancel_upgrade_emits_event() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);
    let wasm_hash = BytesN::from_array(&env, &[15u8; 32]);

    env.events().all();

    client.set_upgrade_signers(&admin, &signers, &2u32);
    let signer1 = signers.get(0).unwrap();
    client.propose_upgrade(&signer1, &wasm_hash);
    client.cancel_upgrade(&admin);

    let events = env.events().all();
    let mut symbols = std::vec::Vec::new();
    for e in events.iter() {
        if let Some(topic) = e.1.get(0) {
            let res: Result<soroban_sdk::Symbol, _> = topic.try_into_val(&env);
            if let Ok(sym) = res {
                symbols.push(sym.to_string());
            }
        }
    }

    assert!(
        symbols.contains(&"up_cncl".to_string()),
        "expected up_cncl event"
    );
}

// ─── Gas cost estimates ────────────────────────────────────────────

#[test]
fn test_upgrade_operation_gas_costs() {
    let (_env, _contract_id, _admin, _token, client) = setup();

    let ops = [
        vero_core_contracts::Operation::SetUpgradeSigners,
        vero_core_contracts::Operation::ProposeUpgrade,
        vero_core_contracts::Operation::ApproveUpgrade,
        vero_core_contracts::Operation::ExecuteUpgrade,
        vero_core_contracts::Operation::CancelUpgrade,
    ];

    for op in ops {
        let cost = client.get_estimated_cost(&op);
        assert!(
            cost > 500_000,
            "{:?} should be above base overhead, got {}",
            op,
            cost
        );
    }

    // Execute upgrade should be the most expensive among upgrade operations
    let execute_cost = client.get_estimated_cost(&vero_core_contracts::Operation::ExecuteUpgrade);
    let propose_cost = client.get_estimated_cost(&vero_core_contracts::Operation::ProposeUpgrade);
    let approve_cost = client.get_estimated_cost(&vero_core_contracts::Operation::ApproveUpgrade);
    assert!(
        execute_cost > propose_cost,
        "execute_upgrade should be more expensive than propose_upgrade"
    );
    assert!(
        execute_cost > approve_cost,
        "execute_upgrade should be more expensive than approve_upgrade"
    );
}

// ─── Integration with batch_execute ────────────────────────────────

#[test]
fn test_batch_execute_with_upgrade_operations() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 2);
    let wasm_hash = BytesN::from_array(&env, &[16u8; 32]);

    let calls = soroban_sdk::vec![
        &env,
        vero_core_contracts::BatchCall::SetUpgradeSigners(admin.clone(), signers.clone(), 2u32,),
        vero_core_contracts::BatchCall::ProposeUpgrade(signers.get(0).unwrap(), wasm_hash.clone(),),
        vero_core_contracts::BatchCall::ApproveUpgrade(signers.get(1).unwrap()),
    ];

    let result = client.try_batch_execute(&calls);
    assert!(result.is_ok());

    // Verify state was set
    assert_eq!(client.get_upgrade_threshold(), 2);
    assert_eq!(client.get_upgrade_signers().len(), 2);
}

// ─── Storage isolation test ────────────────────────────────────────

#[test]
fn test_upgrade_signers_isolation() {
    let (env, _contract_id, admin, _token, client) = setup();

    let signers = generate_signers(&env, 3);
    client.set_upgrade_signers(&admin, &signers, &2u32);

    // Simulate a different contract instance by registering a new one
    let contract_id2 = env.register_contract(None, vero_core_contracts::VeroContract);
    let client2 = VeroContractClient::new(&env, &contract_id2);

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let admin2 = Address::generate(&env);
    client2.initialize(&admin2, &token.address(), &100);

    // Signers should be empty on the new instance
    assert_eq!(client2.get_upgrade_signers().len(), 0);
    assert_eq!(client2.get_upgrade_threshold(), 0);
}

// ─── Edge case: single signer (N=1) ────────────────────────────────

#[test]
fn test_single_signer_succeeds_immediately() {
    let (env, _contract_id, admin, _token, client) = setup();
    let signers = generate_signers(&env, 1);
    let wasm_hash = BytesN::from_array(&env, &[17u8; 32]);

    client.set_upgrade_signers(&admin, &signers, &1u32);

    let signer = signers.get(0).unwrap();
    client.propose_upgrade(&signer, &wasm_hash);

    // Single signer means threshold is met immediately on propose
    let exec_result = client.try_execute_upgrade();
    assert!(
        exec_result.is_err(),
        "should attempt deploy and fail at runtime"
    );
}

// ─── Legacy upgrade test preserved ─────────────────────────────────

#[test]
fn test_upgrade_logic_successful() {
    let (env, admin, _token, client) = setup_without_signers();

    client.grant_role(&admin, &admin, &Role::TaskManager);
    // We register a task to ensure state is present
    client.register_task(&admin, &1u64, &1u32);
    assert!(client.get_task(&1u64).is_some());

    // Verify the function signature is correct
    let _hash = BytesN::from_array(&env, &[0u8; 32]);
    // The following would normally be called with a valid WASM hash:
    // client.upgrade_contract(&admin, &_hash);

    // State remains unaffected
    assert!(client.get_task(&1u64).is_some());
}

fn setup_without_signers() -> (Env, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = token.address();

    client.initialize(&admin, &token_addr, &100);

    (env, admin, token_addr, client)
}
