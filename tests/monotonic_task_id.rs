#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{vec, Address, Env};
use vero_core_contracts::{BatchCall, ContractError, Role, VeroContractClient};

fn setup() -> (Env, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    client.initialize(&admin, &token, &100i128);
    client.grant_role(&admin, &admin, &Role::TaskManager);

    (env, admin, token, client)
}

#[test]
fn test_task_counter_starts_at_zero() {
    let (_env, _admin, _token, client) = setup();
    assert_eq!(client.get_task_counter(), 0);
}

#[test]
fn test_register_task_increments_counter_sequentially() {
    let (_env, admin, _token, client) = setup();

    client.register_task(&admin, &1u64, &1u32);
    assert_eq!(client.get_task_counter(), 1);
    assert!(client.get_task(&1u64).is_some());

    client.register_task(&admin, &2u64, &1u32);
    assert_eq!(client.get_task_counter(), 2);
    assert!(client.get_task(&2u64).is_some());

    client.register_task(&admin, &3u64, &1u32);
    assert_eq!(client.get_task_counter(), 3);
    assert!(client.get_task(&3u64).is_some());
}

#[test]
fn test_register_task_rejects_gap() {
    let (_env, admin, _token, client) = setup();

    // Trying to start at ID 2 (gap when counter is 0) must be rejected
    let res = client.try_register_task(&admin, &2u64, &1u32);
    assert!(matches!(res, Err(Ok(ContractError::InvalidConfig))));
    assert_eq!(client.get_task_counter(), 0);

    // Register 1 correctly
    client.register_task(&admin, &1u64, &1u32);
    assert_eq!(client.get_task_counter(), 1);

    // Trying to jump from 1 to 3 (gap) must be rejected
    let res = client.try_register_task(&admin, &3u64, &1u32);
    assert!(matches!(res, Err(Ok(ContractError::InvalidConfig))));
    assert_eq!(client.get_task_counter(), 1);
    assert!(client.get_task(&3u64).is_none());

    // Register 2 correctly
    client.register_task(&admin, &2u64, &1u32);
    assert_eq!(client.get_task_counter(), 2);
    assert!(client.get_task(&2u64).is_some());
}

#[test]
fn test_register_task_rejects_zero() {
    let (_env, admin, _token, client) = setup();

    let res = client.try_register_task(&admin, &0u64, &1u32);
    assert!(matches!(res, Err(Ok(ContractError::InvalidConfig))));
    assert_eq!(client.get_task_counter(), 0);
}

#[test]
fn test_register_task_rejects_duplicate_or_lower_id() {
    let (_env, admin, _token, client) = setup();

    client.register_task(&admin, &1u64, &1u32);
    assert_eq!(client.get_task_counter(), 1);

    // Re-registering 1 must be rejected
    let res = client.try_register_task(&admin, &1u64, &1u32);
    assert!(matches!(res, Err(Ok(ContractError::InvalidConfig))));
    assert_eq!(client.get_task_counter(), 1);

    client.register_task(&admin, &2u64, &1u32);
    assert_eq!(client.get_task_counter(), 2);

    // Registering 1 again when counter is 2 must be rejected
    let res = client.try_register_task(&admin, &1u64, &1u32);
    assert!(matches!(res, Err(Ok(ContractError::InvalidConfig))));
    assert_eq!(client.get_task_counter(), 2);
}

#[test]
fn test_sequential_registration_lifecycle() {
    let (_env, admin, _token, client) = setup();

    // Register tasks 1, 2, 3 sequentially
    client.register_task(&admin, &1u64, &1u32);
    assert_eq!(client.get_task_counter(), 1);
    client.register_task(&admin, &2u64, &1u32);
    assert_eq!(client.get_task_counter(), 2);
    client.register_task(&admin, &3u64, &1u32);
    assert_eq!(client.get_task_counter(), 3);

    // Attempting to skip to 5 must fail with InvalidConfig
    let res = client.try_register_task(&admin, &5u64, &1u32);
    assert!(matches!(res, Err(Ok(ContractError::InvalidConfig))));
    assert_eq!(client.get_task_counter(), 3);

    // Attempting to reuse 2 must fail
    let res = client.try_register_task(&admin, &2u64, &1u32);
    assert!(matches!(res, Err(Ok(ContractError::InvalidConfig))));
    assert_eq!(client.get_task_counter(), 3);

    // Register 4 succeeds
    client.register_task(&admin, &4u64, &1u32);
    assert_eq!(client.get_task_counter(), 4);
    assert!(client.get_task(&4).is_some());
}

#[test]
fn test_purged_task_cannot_be_reused() {
    let (_env, admin, _token, client) = setup();

    client.register_task(&admin, &1u64, &1u32);
    assert_eq!(client.get_task_counter(), 1);

    // Cancel and purge task 1
    client.cancel_task(&admin, &1u64);
    client.purge_task(&admin, &1u64);
    assert!(client.get_task(&1u64).is_none());

    // TaskCounter should still be 1 (counter never decrements upon purge)
    assert_eq!(client.get_task_counter(), 1);

    // Attempting to re-register task 1 must be rejected to prevent ID collision
    let res = client.try_register_task(&admin, &1u64, &1u32);
    assert!(matches!(res, Err(Ok(ContractError::InvalidConfig))));
    assert_eq!(client.get_task_counter(), 1);

    // Registering the next sequential ID (2) succeeds
    client.register_task(&admin, &2u64, &1u32);
    assert_eq!(client.get_task_counter(), 2);
    assert!(client.get_task(&2u64).is_some());
}

#[test]
fn test_multi_instance_task_counters_are_isolated() {
    let env = Env::default();
    env.mock_all_auths();

    let admin_a = Address::generate(&env);
    let admin_b = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin_a.clone())
        .address();

    let contract_a = env.register_contract(None, vero_core_contracts::VeroContract);
    let client_a = VeroContractClient::new(&env, &contract_a);
    client_a.initialize(&admin_a, &token, &100i128);
    client_a.grant_role(&admin_a, &admin_a, &Role::TaskManager);

    let contract_b = env.register_contract(None, vero_core_contracts::VeroContract);
    let client_b = VeroContractClient::new(&env, &contract_b);
    client_b.initialize(&admin_b, &token, &100i128);
    client_b.grant_role(&admin_b, &admin_b, &Role::TaskManager);

    // Both start at 0
    assert_eq!(client_a.get_task_counter(), 0);
    assert_eq!(client_b.get_task_counter(), 0);

    // Register 1 on contract A
    client_a.register_task(&admin_a, &1u64, &1u32);
    assert_eq!(client_a.get_task_counter(), 1);
    assert_eq!(client_b.get_task_counter(), 0);

    // Register 1 on contract B
    client_b.register_task(&admin_b, &1u64, &1u32);
    assert_eq!(client_a.get_task_counter(), 1);
    assert_eq!(client_b.get_task_counter(), 1);
}

#[test]
fn test_batch_execute_sequential_and_gap_atomic_rollback() {
    let (env, admin, _token, client) = setup();

    // A batch containing a gap on task registration (2 when counter is 0)
    let bad_batch = vec![&env, BatchCall::RegisterTask(admin.clone(), 2u64, 1u32)];
    let res = client.try_batch_execute(&bad_batch);
    assert!(matches!(res, Err(Ok(ContractError::InvalidConfig))));

    // Atomic rollback: task 2 must not exist, counter must remain 0
    assert_eq!(client.get_task_counter(), 0);
    assert!(client.get_task(&2u64).is_none());

    // Valid registration of task 1 via batch_execute succeeds
    let valid_batch = vec![&env, BatchCall::RegisterTask(admin.clone(), 1u64, 1u32)];
    let res = client.try_batch_execute(&valid_batch);
    assert!(res.is_ok());
    assert_eq!(client.get_task_counter(), 1);
    assert!(client.get_task(&1u64).is_some());

    // Another batch with gap (attempting 3 when counter is 1)
    let gap_batch = vec![&env, BatchCall::RegisterTask(admin.clone(), 3u64, 1u32)];
    let res = client.try_batch_execute(&gap_batch);
    assert!(matches!(res, Err(Ok(ContractError::InvalidConfig))));
    assert_eq!(client.get_task_counter(), 1);
    assert!(client.get_task(&3u64).is_none());

    // Valid next sequential ID (2) succeeds
    let valid_next = vec![&env, BatchCall::RegisterTask(admin.clone(), 2u64, 1u32)];
    assert!(client.try_batch_execute(&valid_next).is_ok());
    assert_eq!(client.get_task_counter(), 2);
    assert!(client.get_task(&2u64).is_some());
}

#[test]
fn test_register_task_auth_and_pause_does_not_advance_counter() {
    let (env, admin, _token, client) = setup();
    let unauthorized_caller = Address::generate(&env);

    // Caller without TaskManager role cannot register task
    let res = client.try_register_task(&unauthorized_caller, &1u64, &1u32);
    assert!(matches!(res, Err(Ok(ContractError::NotAuthorized))));
    assert_eq!(client.get_task_counter(), 0);

    // Grant EmergencyManager to admin so pause works
    client.grant_role(&admin, &admin, &Role::EmergencyManager);

    // Pause contract
    client.pause(&admin);

    // While paused, even admin cannot register task
    let res = client.try_register_task(&admin, &1u64, &1u32);
    assert!(matches!(res, Err(Ok(ContractError::ContractPaused))));
    assert_eq!(client.get_task_counter(), 0);

    // Unpause contract
    client.unpause(&admin);

    // Registration now succeeds and counter advances cleanly
    client.register_task(&admin, &1u64, &1u32);
    assert_eq!(client.get_task_counter(), 1);
    assert!(client.get_task(&1u64).is_some());
}
