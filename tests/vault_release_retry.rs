#![cfg(test)]

use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env};
use vero_core_contracts::{ContractError, Role, VeroContract, VeroContractClient};

/// Configurable mock vault contract that can simulate failures, count calls, and succeed on recovery.
#[contract]
pub struct MockVaultContract;

#[contractimpl]
impl MockVaultContract {
    pub fn set_should_fail(env: Env, should_fail: bool) {
        let key = soroban_sdk::Symbol::new(&env, "fail");
        env.storage().instance().set(&key, &should_fail);
    }

    pub fn release_funds(env: Env, _task_id: u64) {
        let fail_key = soroban_sdk::Symbol::new(&env, "fail");
        let should_fail: bool = env.storage().instance().get(&fail_key).unwrap_or(false);
        if should_fail {
            panic!("Vault release failed");
        }

        let calls_key = soroban_sdk::Symbol::new(&env, "calls");
        let count: u32 = env.storage().instance().get(&calls_key).unwrap_or(0);
        env.storage().instance().set(&calls_key, &(count + 1));
    }

    pub fn get_call_count(env: Env) -> u32 {
        let key = soroban_sdk::Symbol::new(&env, "calls");
        env.storage().instance().get(&key).unwrap_or(0)
    }
}

fn setup() -> (Env, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    client.initialize(&admin, &token_addr, &100i128);
    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.grant_role(&admin, &admin, &Role::TaskManager);
    client.grant_role(&admin, &admin, &Role::ConfigManager);
    client.grant_role(&admin, &admin, &Role::EmergencyManager);

    (env, admin, token_addr, client)
}

fn add_guardian_and_lock(
    env: &Env,
    client: &VeroContractClient<'static>,
    admin: &Address,
    token: &Address,
    rep: u64,
) -> Address {
    let guardian = Address::generate(env);
    client.add_guardian(admin, &guardian);
    client.set_reputation(admin, &guardian, &rep);

    let sac = soroban_sdk::token::StellarAssetClient::new(env, token);
    sac.mint(&guardian, &200i128);
    client.lock_tokens(&guardian, &150i128);
    guardian
}

#[test]
fn test_task_resolves_and_records_pending_release_when_vault_fails() {
    let (env, admin, token, client) = setup();
    let vault_id = env.register_contract(None, MockVaultContract);
    let vault_client = MockVaultContractClient::new(&env, &vault_id);
    vault_client.set_should_fail(&true);

    client.set_vault_address(&admin, &vault_id);
    client.set_weight_threshold(&admin, &100u64);
    client.register_task(&admin, &1u64, &1u32);

    let guardian = add_guardian_and_lock(&env, &client, &admin, &token, 100);

    // Vote resolves the task
    client.vote(&guardian, &1u64);

    // 1. Acceptance Criterion 1: Task resolves even when vault call fails
    let task = client.get_task(&1u64).unwrap();
    assert!(task.is_done, "Task must resolve even if vault fails");
    assert_eq!(task.votes, 1);

    // 2. Acceptance Criterion 2: Task is recorded as having an outstanding release
    let pending = client.get_pending_releases();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending.get(0).unwrap(), 1u64);
}

#[test]
fn test_successful_vault_release_does_not_create_pending_release() {
    let (env, admin, token, client) = setup();
    let vault_id = env.register_contract(None, MockVaultContract);
    let vault_client = MockVaultContractClient::new(&env, &vault_id);
    vault_client.set_should_fail(&false);

    client.set_vault_address(&admin, &vault_id);
    client.set_weight_threshold(&admin, &100u64);
    client.register_task(&admin, &1u64, &1u32);

    let guardian = add_guardian_and_lock(&env, &client, &admin, &token, 100);

    client.vote(&guardian, &1u64);

    let task = client.get_task(&1u64).unwrap();
    assert!(task.is_done);

    // No pending release recorded because vault release succeeded
    let pending = client.get_pending_releases();
    assert_eq!(pending.len(), 0);

    // Attempting retry on non-pending task is rejected
    let res = client.try_retry_vault_release(&1u64);
    assert!(matches!(res, Err(Ok(ContractError::NoPendingRelease))));
}

#[test]
fn test_retry_after_vault_recovers_releases_funds_and_clears_record() {
    let (env, admin, token, client) = setup();
    let vault_id = env.register_contract(None, MockVaultContract);
    let vault_client = MockVaultContractClient::new(&env, &vault_id);
    vault_client.set_should_fail(&true);

    client.set_vault_address(&admin, &vault_id);
    client.set_weight_threshold(&admin, &100u64);
    client.register_task(&admin, &42u64, &1u32);

    let guardian = add_guardian_and_lock(&env, &client, &admin, &token, 100);
    client.vote(&guardian, &42u64);

    assert_eq!(client.get_pending_releases().len(), 1);

    // Vault recovers: configure it to succeed
    vault_client.set_should_fail(&false);

    // 3. Acceptance Criterion 3: A retry after vault recovers releases funds and clears record
    client.retry_vault_release(&42u64);

    let pending = client.get_pending_releases();
    assert_eq!(
        pending.len(),
        0,
        "Pending release must be cleared after successful retry"
    );

    // Verify mock vault received the release call
    assert_eq!(vault_client.get_call_count(), 1);
}

#[test]
fn test_retry_for_task_with_no_pending_release_is_rejected() {
    let (_env, _admin, _token, client) = setup();

    // 4. Acceptance Criterion 4: A retry for a task with no pending release is rejected
    let res = client.try_retry_vault_release(&999u64);
    assert!(matches!(res, Err(Ok(ContractError::NoPendingRelease))));
}

#[test]
fn test_retrying_twice_does_not_double_release() {
    let (env, admin, token, client) = setup();
    let vault_id = env.register_contract(None, MockVaultContract);
    let vault_client = MockVaultContractClient::new(&env, &vault_id);
    vault_client.set_should_fail(&true);

    client.set_vault_address(&admin, &vault_id);
    client.set_weight_threshold(&admin, &100u64);
    client.register_task(&admin, &10u64, &1u32);

    let guardian = add_guardian_and_lock(&env, &client, &admin, &token, 100);
    client.vote(&guardian, &10u64);

    // Configure recovered working vault
    vault_client.set_should_fail(&false);

    // First retry succeeds
    client.retry_vault_release(&10u64);

    // 5. Acceptance Criterion 5: Retrying twice does not double-release
    let second_retry = client.try_retry_vault_release(&10u64);
    assert!(matches!(
        second_retry,
        Err(Ok(ContractError::NoPendingRelease))
    ));

    assert_eq!(
        vault_client.get_call_count(),
        1,
        "Vault release must not be called a second time"
    );
}

#[test]
fn test_retry_fails_when_vault_still_unhealthy_preserves_record() {
    let (env, admin, token, client) = setup();
    let vault_id = env.register_contract(None, MockVaultContract);
    let vault_client = MockVaultContractClient::new(&env, &vault_id);
    vault_client.set_should_fail(&true);

    client.set_vault_address(&admin, &vault_id);
    client.set_weight_threshold(&admin, &100u64);
    client.register_task(&admin, &77u64, &1u32);

    let guardian = add_guardian_and_lock(&env, &client, &admin, &token, 100);
    client.vote(&guardian, &77u64);

    assert_eq!(client.get_pending_releases().len(), 1);

    // Retry while vault is still failing
    let res = client.try_retry_vault_release(&77u64);
    assert!(matches!(res, Err(Ok(ContractError::EscrowUnavailable))));

    // Record is preserved for future retry when vault recovers
    assert_eq!(client.get_pending_releases().len(), 1);
    assert_eq!(client.get_pending_releases().get(0).unwrap(), 77u64);
}

#[test]
fn test_purging_task_cleans_up_pending_release() {
    let (env, admin, token, client) = setup();
    let vault_id = env.register_contract(None, MockVaultContract);
    let vault_client = MockVaultContractClient::new(&env, &vault_id);
    vault_client.set_should_fail(&true);

    client.set_vault_address(&admin, &vault_id);
    client.set_weight_threshold(&admin, &100u64);
    client.register_task(&admin, &5u64, &1u32);

    let guardian = add_guardian_and_lock(&env, &client, &admin, &token, 100);
    client.vote(&guardian, &5u64);

    assert_eq!(client.get_pending_releases().len(), 1);

    // Purge terminal task
    client.purge_task(&admin, &5u64);

    // Pending release index and record cleaned up
    assert_eq!(client.get_pending_releases().len(), 0);
    let res = client.try_retry_vault_release(&5u64);
    assert!(matches!(res, Err(Ok(ContractError::NoPendingRelease))));
}

#[test]
fn test_retry_vault_release_paused_rejected() {
    let (env, admin, token, client) = setup();
    let vault_id = env.register_contract(None, MockVaultContract);
    let vault_client = MockVaultContractClient::new(&env, &vault_id);
    vault_client.set_should_fail(&true);

    client.set_vault_address(&admin, &vault_id);
    client.set_weight_threshold(&admin, &100u64);
    client.register_task(&admin, &88u64, &1u32);

    let guardian = add_guardian_and_lock(&env, &client, &admin, &token, 100);
    client.vote(&guardian, &88u64);

    // Pause contract
    client.pause(&admin);

    let res = client.try_retry_vault_release(&88u64);
    assert!(matches!(res, Err(Ok(ContractError::ContractPaused))));
}

#[test]
fn test_batch_voting_records_pending_release_on_failure() {
    let (env, admin, token, client) = setup();
    let vault_id = env.register_contract(None, MockVaultContract);
    let vault_client = MockVaultContractClient::new(&env, &vault_id);
    vault_client.set_should_fail(&true);

    client.set_vault_address(&admin, &vault_id);
    client.set_weight_threshold(&admin, &100u64);
    client.register_task(&admin, &101u64, &1u32);
    client.register_task(&admin, &102u64, &1u32);

    let guardian = add_guardian_and_lock(&env, &client, &admin, &token, 100);

    let batch = soroban_sdk::vec![&env, 101u64, 102u64];
    client.vote_batch(&guardian, &batch);

    let pending = client.get_pending_releases();
    assert_eq!(pending.len(), 2);
    assert!(pending.contains(101u64));
    assert!(pending.contains(102u64));

    // Now vault recovers
    vault_client.set_should_fail(&false);

    // Retry 101
    client.retry_vault_release(&101u64);
    let pending_after_first = client.get_pending_releases();
    assert_eq!(pending_after_first.len(), 1);
    assert_eq!(pending_after_first.get(0).unwrap(), 102u64);

    // Retry 102
    client.retry_vault_release(&102u64);
    assert_eq!(client.get_pending_releases().len(), 0);
}
