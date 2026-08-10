#![cfg(test)]

use soroban_sdk::token::StellarAssetClient as TestTokenClient;
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env};
use vero_core_contracts::{Role, VeroContract, VeroContractClient};

#[contract]
pub struct MockDripsContract;

#[contractimpl]
impl MockDripsContract {
    pub fn start_stream(_env: Env, _contributor: Address, _task_id: u64, _resolution_status: u32) {}
}

#[test]
fn test_end_to_end_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    // 1. Initialize the contract with an admin address
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    let contract_id = env.register_contract(None, VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let lock_threshold = 100i128;
    client.initialize(&admin, &token_addr, &lock_threshold);

    assert_eq!(client.get_admin(), Some(admin.clone()));

    // 2. Grant the necessary roles and add a guardian
    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.grant_role(&admin, &admin, &Role::TaskManager);
    client.grant_role(&admin, &admin, &Role::TreasuryManager);

    assert!(client.has_role(&admin, &Role::GuardianManager));
    assert!(client.has_role(&admin, &Role::TaskManager));
    assert!(client.has_role(&admin, &Role::TreasuryManager));

    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);
    assert!(client.is_guardian(&guardian));

    // 3. Set user reputation and lock tokens above the required threshold
    client.set_reputation(&admin, &guardian, &300u64);
    assert_eq!(client.get_reputation(&guardian), Some(300));

    let sac = TestTokenClient::new(&env, &token_addr);
    sac.mint(&guardian, &105i128);
    client.lock_tokens(&guardian, &105i128);

    // 4. Register a task
    let task_id = 42u64;
    client.register_task(&admin, &task_id, &1u32);

    let task = client.get_task(&task_id).unwrap();
    assert_eq!(task.id, task_id);
    assert_eq!(task.votes, 0);
    assert!(!task.is_done);

    // 5. Cast enough votes to cross the weight threshold and minimum required votes
    client.vote(&guardian, &task_id);

    // 6. Assert that the task's completion flag (is_done) flips to true
    let task_after = client.get_task(&task_id).unwrap();
    assert_eq!(task_after.votes, 1);
    assert_eq!(task_after.total_weight_accrued, 300);
    assert!(task_after.is_done);

    // 7. Start a reward stream for the user and assert it is correctly queryable via get_reward_stream
    let contributor = Address::generate(&env);
    let drips_contract_id = env.register_contract(None, MockDripsContract);

    client.start_reward_stream(&admin, &drips_contract_id, &contributor, &task_id);

    let stream = client.get_reward_stream(&task_id).unwrap();
    assert_eq!(stream.task_id, task_id);
    assert_eq!(stream.contributor, contributor);
    assert_eq!(stream.drips_contract, drips_contract_id);
    assert!(stream.active);
}
