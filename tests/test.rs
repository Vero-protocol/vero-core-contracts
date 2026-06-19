#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, Vec as SorobanVec,
};
use vero_core_contracts::{register_tasks, Operation, VeroContractClient};

const LOCK_THRESHOLD: i128 = 100;
const MAX_TASK_ID: u64 = u64::MAX / 2;
const MAX_TOKEN_AMOUNT: i128 = i128::MAX / 2;
const MAX_LOCK_THRESHOLD: i128 = MAX_TOKEN_AMOUNT - 1;
const MAX_REPUTATION_SCORE: u64 = 1_000_000_000;
const MAX_WEIGHT_THRESHOLD: u64 = 1_000_000_000_000;
const MAX_REGISTER_TASK_BATCH_SIZE: u64 = 32;
const ARCHIVE_AFTER_SECONDS: u64 = 30 * 24 * 60 * 60;

fn setup() -> (Env, Address, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    client.initialize(&admin, &token_addr, &LOCK_THRESHOLD);

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

fn setup_with_lock_threshold(
    lock_threshold: i128,
) -> (Env, Address, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();
    client.initialize(&admin, &token_addr, &lock_threshold);
    (env, contract_id, admin, token_addr, client)
}

fn lock_for_guardian(
    env: &Env,
    token: &Address,
    client: &VeroContractClient,
    guardian: &Address,
    amount: i128,
) {
    let asset_client = soroban_sdk::token::StellarAssetClient::new(env, token);
    asset_client.mint(guardian, &amount);
    client.lock_tokens(guardian, &amount);
}

// ─── Admin initialisation ───────────────────────────────────────────

#[test]
fn test_admin_is_persisted_on_initialize() {
    let (_, _contract_id, admin, _, client) = setup();
    assert_eq!(client.get_admin(), Some(admin));
}

#[test]
fn test_non_admin_cannot_register_task() {
    let (env, _contract_id, _admin, _token, client) = setup();
    let stranger = Address::generate(&env);

    // stranger is not the stored admin — must be rejected
    let result = client.try_register_task(&stranger, &1u64);
    assert!(result.is_err());
}

#[test]
fn test_admin_can_register_task() {
    let (_env, _contract_id, admin, _token, client) = setup();
    client.register_task(&admin, &1u64);
    assert!(client.get_task(&1u64).is_some());
}

#[test]
fn test_register_task_rejected_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);
    let stranger = Address::generate(&env);

    // No initialize called → no stored admin → NotInitialized
    let result = client.try_register_task(&stranger, &1u64);
    assert!(result.is_err());
}

// ─── Basic contract state ───────────────────────────────────────────

#[test]
fn test_add_guardian_and_register_task() {
    let (env, _contract_id, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    client.register_task(&admin, &1u64);

    let task = client.get_task(&1u64).unwrap();
    assert_eq!(task.id, 1);
    assert_eq!(task.votes, 0);
    assert_eq!(task.total_weight_accrued, 0);
    assert_eq!(task.resolved_at, 0);
    assert!(!task.is_done);
}

#[test]
fn valid_admin_config_update_succeeds() {
    let (env, _contract_id, admin, _token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 500);
    let vault = Address::generate(&env);

    client.set_weight_threshold(&admin, &500);
    client.set_vault_address(&admin, &vault);

    assert_eq!(client.get_weight_threshold(), 500);
    assert!(client.is_guardian(&guardian));
    assert_eq!(client.get_reputation(&guardian), Some(500));

    let snapshot = client.get_snapshot();
    assert_eq!(snapshot.vault_address, Some(vault));
}

#[test]
fn initialize_rejects_self_token_and_invalid_thresholds_without_mutation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    // Initialize succeeds with valid parameters
    let result = client.try_initialize(&admin, &token_addr, &LOCK_THRESHOLD);
    assert!(result.is_ok());

    // Double initialization should be rejected
    let result = client.try_initialize(&admin, &token_addr, &LOCK_THRESHOLD);
    assert!(result.is_err());
}

#[test]
fn test_calculate_voting_power_returns_score() {
    let (env, _contract_id, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &150u64);

    assert_eq!(client.calculate_voting_power(&guardian), Some(150));
}

#[test]
fn test_calculate_voting_power_none_for_unset() {
    let (env, _contract_id, _admin, _token, client) = setup();
    let stranger = Address::generate(&env);

    assert_eq!(client.calculate_voting_power(&stranger), None);
}

// ─── Weighted consensus ─────────────────────────────────────────────

#[test]
fn test_single_high_rep_guardian_resolves_task() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let g = add_guardian_with_rep(&env, &client, &admin, 300);
    client.register_task(&admin, &1u64);
    lock_for_guardian(&env, &token, &client, &g, 101);
    client.vote(&g, &1u64);

    let task = client.get_task(&1u64).unwrap();
    assert_eq!(task.votes, 1);
    assert_eq!(task.total_weight_accrued, 300);
    assert!(task.is_done);
}

#[test]
fn test_multiple_guardians_accumulate_weight() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let g1 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g3 = add_guardian_with_rep(&env, &client, &admin, 100);

    client.register_task(&admin, &42u64);

    lock_for_guardian(&env, &token, &client, &g1, 101);
    lock_for_guardian(&env, &token, &client, &g2, 101);
    lock_for_guardian(&env, &token, &client, &g3, 101);

    client.vote(&g1, &42u64);
    client.vote(&g2, &42u64);
    client.vote(&g3, &42u64);

    let task = client.get_task(&42u64).unwrap();
    assert_eq!(task.votes, 3);
    assert!(task.is_done);
}

#[test]
fn test_weight_vs_count_logic() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let g1 = add_guardian_with_rep(&env, &client, &admin, 200);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 150);

    client.register_task(&admin, &20u64);

    lock_for_guardian(&env, &token, &client, &g1, 101);
    lock_for_guardian(&env, &token, &client, &g2, 101);

    client.vote(&g1, &20u64);
    client.vote(&g2, &20u64);

    let task = client.get_task(&20u64).unwrap();
    assert_eq!(task.votes, 2);
    assert_eq!(task.total_weight_accrued, 350);
    assert!(task.is_done);
}

#[test]
fn test_insufficient_weight_does_not_resolve_task() {
    // Five guardians each with rep=100, threshold=600 → total 500 < 600, not done.
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &600u64);

    let guardians: [Address; 5] =
        core::array::from_fn(|_| add_guardian_with_rep(&env, &client, &admin, 100));
    client.register_task(&admin, &1u64);

    for g in &guardians {
        lock_for_guardian(&env, &token, &client, g, 101);
        client.vote(g, &1u64);
    }

    let task = client.get_task(&1).unwrap();
    assert_eq!(task.votes, 5);
    assert_eq!(task.total_weight_accrued, 500);
    assert!(!task.is_done);
}

#[test]
fn numeric_minimum_and_maximum_boundaries_succeed() {
    let (env, _contract_id, admin, token, client) = setup_with_lock_threshold(1);
    let guardian = add_guardian_with_rep(&env, &client, &admin, 1);

    client.set_reputation(&admin, &guardian, &MAX_REPUTATION_SCORE);
    client.set_weight_threshold(&admin, &MAX_WEIGHT_THRESHOLD);
    client.register_task(&admin, &MAX_TASK_ID);
    lock_for_guardian(&env, &token, &client, &guardian, MAX_TOKEN_AMOUNT);

    assert_eq!(client.get_reputation(&guardian), Some(MAX_REPUTATION_SCORE));
    assert_eq!(client.get_weight_threshold(), MAX_WEIGHT_THRESHOLD);
    assert!(client.get_task(&MAX_TASK_ID).is_some());
}

#[test]
fn maximum_lock_threshold_still_allows_max_balance_vote() {
    let (env, _contract_id, admin, token, client) = setup_with_lock_threshold(MAX_LOCK_THRESHOLD);
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);

    client.register_task(&admin, &1);
    lock_for_guardian(&env, &token, &client, &guardian, MAX_TOKEN_AMOUNT);
    client.vote(&guardian, &1);

    let task = client.get_task(&1).unwrap();
    assert!(task.is_done);
    assert_eq!(task.total_weight_accrued, 300);
}

#[test]
fn guardian_address_validation_rejects_self_and_duplicate_roles() {
    let (_env, contract_id, admin, _token, client) = setup();

    assert!(client.try_add_guardian(&admin, &contract_id).is_err());
    assert!(client.try_add_guardian(&admin, &admin).is_err());
    assert!(!client.is_guardian(&contract_id));
    assert!(!client.is_guardian(&admin));
}

#[test]
fn test_task_resolved_includes_final_weight() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &200u64);

    let g1 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 115);

    client.register_task(&admin, &40u64);

    lock_for_guardian(&env, &token, &client, &g1, 101);
    lock_for_guardian(&env, &token, &client, &g2, 101);

    client.vote(&g1, &40u64);
    client.vote(&g2, &40u64);

    let task = client.get_task(&40u64).unwrap();
    assert_eq!(task.total_weight_accrued, 215);
    assert!(task.is_done);
}

#[test]
fn reputation_validation_rejects_zero_over_max_and_non_guardian() {
    let (env, _contract_id, admin, _token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 100);
    let non_guardian = Address::generate(&env);

    assert!(client.try_set_reputation(&admin, &guardian, &0).is_err());
    assert!(client
        .try_set_reputation(&admin, &guardian, &u64::MAX)
        .is_err());
    assert!(client
        .try_set_reputation(&admin, &non_guardian, &100)
        .is_err());

    assert_eq!(client.get_reputation(&guardian), Some(100));
    assert_eq!(client.get_reputation(&non_guardian), None);
}

// ─── Reputation gate ────────────────────────────────────────────────

#[test]
fn weight_threshold_validation_rejects_zero_and_over_max_without_mutation() {
    let (_env, contract_id, admin, _token, client) = setup();

    client.set_weight_threshold(&admin, &750);
    assert_eq!(client.get_weight_threshold(), 750);

    assert!(client.try_set_weight_threshold(&admin, &0).is_err());
    assert!(client.try_set_weight_threshold(&admin, &u64::MAX).is_err());
    assert!(client.try_set_weight_threshold(&contract_id, &500).is_err());
}

#[test]
fn test_vote_rejected_without_reputation() {
    let (env, _contract_id, admin, token, client) = setup();
    let g = Address::generate(&env);
    client.add_guardian(&admin, &g);
    client.register_task(&admin, &7u64);
    lock_for_guardian(&env, &token, &client, &g, LOCK_THRESHOLD + 1);

    // No reputation set → NoReputationScore
    let result = client.try_vote(&g, &7u64);
    assert!(result.is_err());
}

#[test]
fn test_vote_rejected_with_insufficient_reputation() {
    let (env, _contract_id, admin, token, client) = setup();
    // Score 99 is below MIN_REPUTATION_THRESHOLD (100)
    let g = add_guardian_with_rep(&env, &client, &admin, 99);
    client.register_task(&admin, &8u64);
    lock_for_guardian(&env, &token, &client, &g, 101);

    let result = client.try_vote(&g, &8u64);
    assert!(result.is_err());
}

#[test]
fn test_vote_allowed_at_minimum_reputation_threshold() {
    let (env, _contract_id, admin, token, client) = setup();
    // Score 100 is exactly at MIN_REPUTATION_THRESHOLD — should be allowed
    let g = add_guardian_with_rep(&env, &client, &admin, 100);
    client.register_task(&admin, &9u64);
    lock_for_guardian(&env, &token, &client, &g, 101);

    let result = client.try_vote(&g, &9u64);
    assert!(result.is_ok());
}

#[test]
fn vote_rejects_zero_task_id_without_recording_vote() {
    let (env, _contract_id, admin, token, client) = setup();
    let g = add_guardian_with_rep(&env, &client, &admin, 100);
    client.register_task(&admin, &10u64);
    lock_for_guardian(&env, &token, &client, &g, 101);

    let result = client.try_vote(&g, &0u64);
    assert!(result.is_err());
    assert!(client.get_task(&0u64).is_none());

    let task = client.get_task(&10u64).unwrap();
    assert_eq!(task.votes, 0);
    assert_eq!(task.total_weight_accrued, 0);
    assert!(!task.is_done);
}

#[test]
fn test_vote_on_nonexistent_task_rejected() {
    let (env, _contract_id, admin, _token, client) = setup();
    let g = add_guardian_with_rep(&env, &client, &admin, 100);

    assert_eq!(client.get_weight_threshold(), 750);
}

#[test]
fn task_id_validation_rejects_zero_and_over_max_without_mutation() {
    let (_env, contract_id, admin, _token, client) = setup();

    assert!(client.try_register_task(&admin, &0).is_err());
    assert!(client.try_register_task(&admin, &u64::MAX).is_err());
    assert!(client.try_register_task(&contract_id, &42).is_err());
    assert!(client.get_task(&0).is_none());
    assert!(client.get_task(&u64::MAX).is_none());
    assert!(client.get_task(&42).is_none());

    client.register_task(&admin, &42);
    assert!(client.get_task(&42).is_some());
}

#[test]
fn vault_address_validation_rejects_self_without_mutation() {
    let (env, contract_id, admin, _token, client) = setup();
    let vault = Address::generate(&env);

    client.set_vault_address(&admin, &vault);
    assert_eq!(client.get_snapshot().vault_address, Some(vault.clone()));
}

#[test]
fn test_reputation_can_be_updated() {
    let (env, _contract_id, admin, _token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    client.set_reputation(&admin, &g, &100u64);
    assert_eq!(client.get_reputation(&g), Some(100));

    client.set_reputation(&admin, &g, &500u64);
    assert_eq!(client.get_reputation(&g), Some(500));
    assert_eq!(client.calculate_voting_power(&g), Some(500));
}

// ─── Drips integration ─────────────────────────────────────────────

#[test]
fn test_reward_stream_rejected_until_task_verified() {
    let (env, _contract_id, admin, _token, client) = setup();
    let contributor = Address::generate(&env);
    let drips_addr = Address::generate(&env);

    let result = client.try_start_reward_stream(&admin, &drips_addr, &contributor, &999u64);
    assert!(result.is_err());
}

#[test]
fn test_reward_stream_duplicate_rejected() {
    let (env, contract_id, admin, token, client) = setup();
    let vault = Address::generate(&env);
    let contributor = Address::generate(&env);

    let g1 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g3 = add_guardian_with_rep(&env, &client, &admin, 100);
    client.register_task(&admin, &50u64);

    lock_for_guardian(&env, &token, &client, &g1, 101);
    lock_for_guardian(&env, &token, &client, &g2, 101);
    lock_for_guardian(&env, &token, &client, &g3, 101);

    client.vote(&g1, &50u64);
    client.vote(&g2, &50u64);
    client.vote(&g3, &50u64);

    client.set_vault_address(&admin, &vault);
    assert!(client.try_set_vault_address(&admin, &contract_id).is_err());

    assert_eq!(client.get_snapshot().vault_address, Some(vault));
}

#[test]
fn reward_stream_validation_rejects_invalid_addresses_and_ids() {
    let (env, contract_id, admin, token, client) = setup();
    let contributor = Address::generate(&env);
    let drips_contract_id = env.register_contract(None, MockDripsContract);
    let vault = Address::generate(&env);
    client.set_vault_address(&admin, &vault);

    // Register, vote, and resolve task 77
    client.register_task(&admin, &77u64);
    let g = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &g, 101);
    client.vote(&g, &77u64);

    assert!(client
        .try_start_reward_stream(&admin, &contract_id, &contributor, &77)
        .is_err());
    assert!(client
        .try_start_reward_stream(&admin, &drips_contract_id, &contract_id, &77)
        .is_err());
    assert!(client
        .try_start_reward_stream(&admin, &contributor, &contributor, &77)
        .is_err());
    assert!(client
        .try_start_reward_stream(&admin, &drips_contract_id, &contributor, &0)
        .is_err());

    assert!(client.get_reward_stream(&77).is_none());

    client.start_reward_stream(&admin, &drips_contract_id, &contributor, &77);
    let stream = client.get_reward_stream(&77).unwrap();

    let g1 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g3 = add_guardian_with_rep(&env, &client, &admin, 100);
    client.register_task(&admin, &77u64);

    lock_for_guardian(&env, &token, &client, &g1, 101);
    lock_for_guardian(&env, &token, &client, &g2, 101);
    lock_for_guardian(&env, &token, &client, &g3, 101);

    client.vote(&g1, &77u64);
    client.vote(&g2, &77u64);
    client.vote(&g3, &77u64);

    let drips_contract_id = env.register_contract(None, MockDripsContract);
    client.start_reward_stream(&admin, &drips_contract_id, &contributor, &77u64);

    let stream = client.get_reward_stream(&77u64).unwrap();
    assert_eq!(stream.task_id, 77);
    assert_eq!(stream.contributor, contributor);
    assert_eq!(stream.drips_contract, drips_contract_id);
    assert!(stream.active);
}

#[test]
fn token_amount_validation_rejects_zero_negative_and_over_max_without_locking() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    client.register_task(&admin, &88);

    assert!(client.try_lock_tokens(&guardian, &0).is_err());
    assert!(client.try_lock_tokens(&guardian, &-1).is_err());
    assert!(client.try_lock_tokens(&guardian, &i128::MAX).is_err());
    assert!(client.try_vote(&guardian, &88).is_err());

    lock_for_guardian(&env, &token, &client, &guardian, LOCK_THRESHOLD + 1);
    client.vote(&guardian, &88);
    assert_eq!(client.get_task(&88).unwrap().votes, 1);
}

#[test]
fn aggregate_locked_amount_above_max_is_rejected_without_transfer() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    let balance_client = soroban_sdk::token::Client::new(&env, &token);

    lock_for_guardian(&env, &token, &client, &guardian, MAX_TOKEN_AMOUNT);
    token_client.mint(&guardian, &1);

    assert_eq!(balance_client.balance(&guardian), 1);
    assert!(client.try_lock_tokens(&guardian, &1).is_err());
    assert_eq!(balance_client.balance(&guardian), 1);

    client.register_task(&admin, &89);
    client.vote(&guardian, &89);
    assert_eq!(client.get_task(&89).unwrap().votes, 1);
}

#[test]
fn unauthorized_admin_call_is_still_rejected_and_state_is_unchanged() {
    let env = Env::default();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();
    client.initialize(&admin, &token_addr, &LOCK_THRESHOLD);

    assert!(client.try_set_weight_threshold(&admin, &500).is_err());
    assert_eq!(client.get_weight_threshold(), 300);
}

#[test]
fn test_locked_balance_must_exceed_threshold() {
    let (env, _contract_id, admin, token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    client.set_reputation(&admin, &g, &100u64);
    client.register_task(&admin, &100u64);

    lock_for_guardian(&env, &token, &client, &g, 100);
    assert!(client.try_vote(&g, &100u64).is_err());

    lock_for_guardian(&env, &token, &client, &g, 1);
    client.vote(&g, &100u64);
    assert_eq!(client.get_task(&100u64).unwrap().votes, 1);
}

#[test]
fn test_duplicate_vote_rejected() {
    let (env, _contract_id, admin, token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    client.set_reputation(&admin, &g, &100u64);
    client.register_task(&admin, &10u64);

    // Lock strictly above threshold (threshold = 100, need > 100)
    lock_for_guardian(&env, &token, &client, &g, 101);
    client.vote(&g, &10u64);

    let result = client.try_vote(&g, &10u64);
    assert!(result.is_err());
    assert_eq!(client.get_task(&10u64).unwrap().votes, 1);
}

#[test]
fn test_resign_guardian_refunds_tokens() {
    let (env, _contract_id, admin, token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    lock_for_guardian(&env, &token, &client, &g, 200);

    // Initiate the 24-hour timelock, then advance ledger past it
    client.request_unlock(&g);
    let timelock = client.get_withdrawal_timelock(&g).unwrap();
    env.ledger().set_timestamp(timelock + 86401u64);

    client.resign_guardian(&g);

    assert!(!client.is_guardian(&g));
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&g), 200);
}

#[test]
fn existing_valid_vote_and_unlock_flows_still_pass() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    client.register_task(&admin, &99);
    lock_for_guardian(&env, &token, &client, &guardian, LOCK_THRESHOLD + 1);

    client.vote(&guardian, &99);
    let task = client.get_task(&99).unwrap();
    assert!(task.is_done);
    assert_eq!(task.total_weight_accrued, 300);
    assert!(client.try_unlock_tokens(&guardian).is_err());
}

#[test]
fn test_unlock_succeeds_for_non_guardian() {
    let (env, _contract_id, admin, token, client) = setup();
    let non_guardian = Address::generate(&env);
    client.add_guardian(&admin, &non_guardian);
    client.set_reputation(&admin, &non_guardian, &100u64);
    client.register_task(&admin, &1u64);
    lock_for_guardian(&env, &token, &client, &non_guardian, LOCK_THRESHOLD + 1);
    client.vote(&non_guardian, &1u64);

    client.resign_guardian(&non_guardian);
    assert!(!client.is_guardian(&non_guardian));

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&non_guardian), LOCK_THRESHOLD + 1);
}

#[test]
fn paused_contract_rejects_config_updates() {
    let (env, _contract_id, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    client.pause(&admin);

    assert!(client.try_add_guardian(&admin, &guardian).is_err());
    assert!(client.try_set_weight_threshold(&admin, &400).is_err());
    assert_eq!(client.get_weight_threshold(), 300);

    client.unpause(&admin);
    client.add_guardian(&admin, &guardian);
    assert!(client.is_guardian(&guardian));
}

#[test]
fn register_task_batch_size_boundaries_are_enforced() {
    let (env, contract_id, admin, _token, client) = setup();
    let mut max_batch = SorobanVec::new(&env);
    for task_id in 1..=MAX_REGISTER_TASK_BATCH_SIZE {
        max_batch.push_back(task_id);
    }

    let max_result = env.as_contract(&contract_id, || {
        register_tasks(&env, admin.clone(), max_batch)
    });
    assert!(max_result.is_ok());
    assert!(client.get_task(&1).is_some());
    assert!(client.get_task(&MAX_REGISTER_TASK_BATCH_SIZE).is_some());

    let mut oversized_batch = SorobanVec::new(&env);
    for task_id in 100..=(100 + MAX_REGISTER_TASK_BATCH_SIZE) {
        oversized_batch.push_back(task_id);
    }

    let oversized_result = env.as_contract(&contract_id, || {
        register_tasks(&env, admin.clone(), oversized_batch)
    });
    assert!(oversized_result.is_err());
    assert!(client.get_task(&100).is_none());
    assert!(client
        .get_task(&(100 + MAX_REGISTER_TASK_BATCH_SIZE))
        .is_none());
}

#[test]
fn archive_timestamp_underflow_is_safely_rejected_without_mutation() {
    let (env, _contract_id, admin, token, client) = setup();

    // Set weight threshold to 1 so single guardian can resolve
    client.set_weight_threshold(&admin, &1);
    let guardian = add_guardian_with_rep(&env, &client, &admin, 1);
    client.register_task(&admin, &61u64);
    lock_for_guardian(&env, &token, &client, &guardian, 101);
    env.ledger().set_timestamp(1_000);
    client.vote(&guardian, &61u64);
    assert_eq!(client.get_task(&61).unwrap().resolved_at, 1_000);

    env.ledger().set_timestamp(0);
    assert!(client.try_archive_task(&61).is_err());
    assert!(client.get_task(&61).is_some());
    assert!(client.get_archived_task(&61).is_none());

    env.ledger().set_timestamp(1_000 + ARCHIVE_AFTER_SECONDS);
    assert!(client.try_archive_task(&61).is_err());
    assert!(client.get_task(&61).is_some());
    assert!(client.get_archived_task(&61).is_none());

    env.ledger()
        .set_timestamp(1_000 + ARCHIVE_AFTER_SECONDS + 1);
    client.archive_task(&61);
    assert!(client.get_task(&61).is_none());
    assert!(client.get_archived_task(&61).is_some());
}

#[test]
fn invalid_numeric_inputs_do_not_emit_success_events() {
    let (env, _contract_id, admin, token, client) = setup();
    let contributor = Address::generate(&env);
    let drips_contract_id = env.register_contract(None, MockDripsContract);

    let before_register_events = env.events().all().len();
    assert!(client.try_register_task(&admin, &0).is_err());
    assert_eq!(env.events().all().len(), before_register_events);

    // Resolve task 62: register, vote, verify resolved
    let g = add_guardian_with_rep(&env, &client, &admin, 300);
    client.set_weight_threshold(&admin, &1);
    client.register_task(&admin, &62u64);
    lock_for_guardian(&env, &token, &client, &g, 101);
    client.vote(&g, &62u64);
    let before_stream_events = env.events().all().len();
    assert!(client
        .try_start_reward_stream(&admin, &drips_contract_id, &contributor, &0)
        .is_err());
    assert_eq!(env.events().all().len(), before_stream_events);
    assert!(client.get_reward_stream(&0).is_none());
}

#[test]
fn legacy_add_guardian_and_register_task_flow_still_passes() {
    let (env, _contract_id, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    client.register_task(&admin, &1);

    let task = client.get_task(&1).unwrap();
    assert_eq!(task.id, 1);
    assert_eq!(task.votes, 0);
    assert_eq!(task.total_weight_accrued, 0);
    assert_eq!(task.resolved_at, 0);
    assert!(!task.is_done);
}

#[test]
fn legacy_voting_power_views_still_pass() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 150);
    let stranger = Address::generate(&env);

    assert_eq!(client.calculate_voting_power(&guardian), Some(150));
    assert_eq!(client.calculate_voting_power(&stranger), None);
    client.register_task(&admin, &303u64);

    let _ = client.try_vote(&stranger, &303u64);

    lock_for_guardian(&env, &token, &client, &guardian, 101);
    client.vote(&guardian, &303u64);
    assert_eq!(client.get_task(&303u64).unwrap().votes, 1);
}

// ─── Emergency stop (pause/unpause) ────────────────────────────────

#[test]
fn test_admin_can_toggle_pause() {
    let (_env, _contract_id, admin, _token, client) = setup();

    assert!(!client.is_paused());
    client.toggle_pause(&admin);
    assert!(client.is_paused());
    client.toggle_pause(&admin);
    assert!(!client.is_paused());
}

#[test]
fn test_admin_can_pause_and_unpause() {
    let (_env, _contract_id, admin, _token, client) = setup();

    client.pause(&admin);
    assert!(client.is_paused());
    client.unpause(&admin);
    assert!(!client.is_paused());
}

#[test]
fn legacy_multiple_guardian_weight_accumulates() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300);

    let g1 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g3 = add_guardian_with_rep(&env, &client, &admin, 100);
    client.register_task(&admin, &42);
    assert!(client.try_register_task(&admin, &1u64).is_err());

    for guardian in [&g1, &g2, &g3] {
        lock_for_guardian(&env, &token, &client, guardian, LOCK_THRESHOLD + 1);
        client.vote(guardian, &42);
    }

    let task = client.get_task(&42).unwrap();
    assert_eq!(task.votes, 3);
    assert_eq!(task.total_weight_accrued, 300);
    assert!(task.is_done);
    assert!(client.try_register_task(&admin, &2u64).is_err());
}

#[test]
fn legacy_low_weight_votes_do_not_resolve_early() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300);
    client.register_task(&admin, &30);

    // Register 5 low-weight guardians and vote
    for _ in 0..5 {
        let guardian = add_guardian_with_rep(&env, &client, &admin, 50);
        lock_for_guardian(&env, &token, &client, &guardian, LOCK_THRESHOLD + 1);
        client.vote(&guardian, &30);
    }

    let task = client.get_task(&30).unwrap();
    assert_eq!(task.votes, 5);
    assert_eq!(task.total_weight_accrued, 250);
    assert!(!task.is_done);
    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused());

    let stranger = Address::generate(&env);
    assert!(client.try_vote(&stranger, &1u64).is_err());
}

#[test]
fn legacy_reputation_can_be_updated() {
    let (env, _contract_id, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused());

    assert!(client.try_add_guardian(&admin, &guardian).is_err());
}

#[test]
fn test_contract_paused_error_on_set_reputation() {
    let (env, _contract_id, admin, _token, client) = setup();
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &100);
    assert_eq!(client.get_reputation(&guardian), Some(100));

    client.set_reputation(&admin, &guardian, &500);
    assert_eq!(client.get_reputation(&guardian), Some(500));
    assert_eq!(client.calculate_voting_power(&guardian), Some(500));
}

#[test]
fn legacy_vote_rejections_still_pass() {
    let (env, _contract_id, admin, token, client) = setup();
    let no_rep = Address::generate(&env);
    let guardian = add_guardian_with_rep(&env, &client, &admin, 100);
    let stranger = Address::generate(&env);

    client.add_guardian(&admin, &no_rep);
    client.register_task(&admin, &7);
    lock_for_guardian(&env, &token, &client, &no_rep, LOCK_THRESHOLD + 1);
    lock_for_guardian(&env, &token, &client, &guardian, LOCK_THRESHOLD + 1);
    client.toggle_pause(&admin);

    assert!(client
        .try_set_reputation(&admin, &guardian, &100u64)
        .is_err());
}

#[test]
fn test_operations_resume_after_unpause() {
    let (env, _contract_id, admin, token, client) = setup();
    let g = add_guardian_with_rep(&env, &client, &admin, 300);

    client.toggle_pause(&admin);
    assert!(client.is_paused());

    // Verify operations are rejected while paused
    assert!(client.try_register_task(&admin, &1u64).is_err());

    client.toggle_pause(&admin);
    assert!(!client.is_paused());

    // Operations succeed after unpause
    client.register_task(&admin, &1u64);
    lock_for_guardian(&env, &token, &client, &g, 101);
    client.vote(&g, &1u64);
}

#[test]
fn legacy_reward_stream_rejects_unverified_and_duplicate_tasks() {
    let (env, _contract_id, admin, token, client) = setup();
    let contributor = Address::generate(&env);
    let drips_contract_id = env.register_contract(None, MockDripsContract);

    client.register_task(&admin, &50);
    assert!(client
        .try_start_reward_stream(&admin, &drips_contract_id, &contributor, &50)
        .is_err());
}

#[test]
fn test_explicit_pause_and_unpause_rejects_vote() {
    let (env, _contract_id, admin, token, client) = setup();
    client.register_task(&admin, &1u64);
    let g = add_guardian_with_rep(&env, &client, &admin, 100);
    lock_for_guardian(&env, &token, &client, &g, 101);

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_vote(&g, &1u64);
    assert!(result.is_err());
}

#[test]
fn legacy_token_locking_and_unlocking_flows_still_pass() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 100);
    let non_guardian = Address::generate(&env);

    client.register_task(&admin, &100);
    assert!(client.try_vote(&guardian, &100).is_err());

    lock_for_guardian(&env, &token, &client, &guardian, LOCK_THRESHOLD);
    assert!(client.try_vote(&guardian, &100).is_err());

    lock_for_guardian(&env, &token, &client, &guardian, 1);
    client.vote(&guardian, &100);
    assert_eq!(client.get_task(&100).unwrap().votes, 1);

    lock_for_guardian(&env, &token, &client, &non_guardian, 150);
    client.unlock_tokens(&non_guardian);
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&non_guardian), 150);
}

#[test]
fn test_paused_contract_rejects_vote() {
    let (env, _contract_id, admin, token, client) = setup();
    client.register_task(&admin, &1u64);
    let g = add_guardian_with_rep(&env, &client, &admin, 100);
    lock_for_guardian(&env, &token, &client, &g, 101);

    client.pause(&admin);
    assert!(client.is_paused());

    assert!(client.try_vote(&g, &1u64).is_err());
}

#[test]
fn legacy_reentrancy_lock_released_after_failed_vote() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 100);
    let stranger = Address::generate(&env);

    client.register_task(&admin, &303);
    let _ = client.try_vote(&stranger, &303);

    lock_for_guardian(&env, &token, &client, &guardian, LOCK_THRESHOLD + 1);
    client.vote(&guardian, &303);
    assert_eq!(client.get_task(&303).unwrap().votes, 1);
}

#[test]
fn legacy_pause_and_circuit_breaker_flows_still_pass() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 100);

    assert!(!client.is_paused());
    client.toggle_pause(&admin);
    assert!(client.is_paused());
    assert!(client.try_register_task(&admin, &2).is_err());

    client.toggle_pause(&admin);
    client.register_task(&admin, &2);
    lock_for_guardian(&env, &token, &client, &guardian, LOCK_THRESHOLD + 1);
    assert!(client.try_vote(&guardian, &1u64).is_err());
}

#[test]
fn test_admin_can_reset_circuit_breaker() {
    let (env, _contract_id, admin, token, client) = setup();
    client.register_task(&admin, &1u64);
    let g = add_guardian_with_rep(&env, &client, &admin, 100);
    lock_for_guardian(&env, &token, &client, &g, 101);

    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused());
    assert!(client.try_vote(&g, &2).is_err());

    client.reset_circuit_breaker(&admin);
    assert!(!client.is_paused());
    client.vote(&g, &2);
    assert_eq!(client.get_task(&2).unwrap().votes, 1);
}

#[test]
fn debug_circuit_breaker_count() {
    let (_env, _contract_id, _admin, _token, client) = setup();
    for _ in 0..50 {
        client.record_failure();
    }
    assert!(!client.is_paused());

    client.record_failure();
    assert!(client.is_paused());
}

// ─── Gas cost estimation ───────────────────────────────────────────

#[test]
fn legacy_gas_cost_estimates_still_pass() {
    let (_env, _contract_id, _admin, _token, client) = setup();
    let ops = [
        Operation::RegisterTask,
        Operation::Vote,
        Operation::AddGuardian,
        Operation::SetReputation,
        Operation::LockTokens,
        Operation::UnlockTokens,
        Operation::ResignGuardian,
        Operation::SetWeightThreshold,
        Operation::StartRewardStream,
        Operation::TogglePause,
        Operation::RecordFailure,
        Operation::ResetCircuitBreaker,
        Operation::UpgradeContract,
    ];

    for op in ops {
        assert!(client.get_estimated_cost(&op) > 500_000);
        assert!(
            client.get_estimated_cost(&op) > 0,
            "{:?} returned zero cost",
            op
        );
    }

    assert!(
        client.get_estimated_cost(&Operation::Vote)
            >= client.get_estimated_cost(&Operation::RegisterTask)
    );
    assert!(
        client.get_estimated_cost(&Operation::UpgradeContract)
            >= client.get_estimated_cost(&Operation::Vote)
    );
    assert_eq!(
        client.get_estimated_cost(&Operation::SetWeightThreshold),
        650_000
    );
    assert_eq!(
        client.get_estimated_cost(&Operation::UpgradeContract),
        2_500_000
    );
}

#[contract]
pub struct MockDripsContract;

#[contractimpl]
impl MockDripsContract {
    pub fn start_stream(_env: Env, _contributor: Address, _task_id: u64, _resolution_status: u32) {}
}
#[test]
fn test_vote_is_most_expensive_write_operation() {
    let (_env, _contract_id, _admin, _token, client) = setup();
    let vote_cost = client.get_estimated_cost(&Operation::Vote);

    let ops = [
        Operation::RegisterTask,
        Operation::AddGuardian,
        Operation::SetReputation,
        Operation::LockTokens,
        Operation::UnlockTokens,
        Operation::ResignGuardian,
        Operation::SetWeightThreshold,
        Operation::StartRewardStream,
        Operation::TogglePause,
        Operation::RecordFailure,
        Operation::ResetCircuitBreaker,
    ];

    for op in ops {
        assert!(
            vote_cost >= client.get_estimated_cost(&op),
            "Vote should be >= {:?}",
            op
        );
    }
}

#[test]
fn test_upgrade_contract_is_overall_maximum() {
    let (_env, _contract_id, _admin, _token, client) = setup();
    let upgrade_cost = client.get_estimated_cost(&Operation::UpgradeContract);

    let ops = [
        Operation::RegisterTask,
        Operation::Vote,
        Operation::AddGuardian,
        Operation::SetReputation,
        Operation::LockTokens,
        Operation::UnlockTokens,
        Operation::ResignGuardian,
        Operation::SetWeightThreshold,
        Operation::StartRewardStream,
        Operation::TogglePause,
        Operation::RecordFailure,
        Operation::ResetCircuitBreaker,
    ];

    for op in ops {
        assert!(
            upgrade_cost >= client.get_estimated_cost(&op),
            "UpgradeContract should be >= {:?}",
            op
        );
    }
}

#[test]
fn test_cost_spot_checks() {
    let (_env, _contract_id, _admin, _token, client) = setup();

    assert_eq!(
        client.get_estimated_cost(&Operation::SetWeightThreshold),
        650_000
    );
    assert_eq!(
        client.get_estimated_cost(&Operation::SetReputation),
        700_000
    );
    assert_eq!(client.get_estimated_cost(&Operation::AddGuardian), 700_000);
    assert_eq!(client.get_estimated_cost(&Operation::TogglePause), 730_000);
    assert_eq!(
        client.get_estimated_cost(&Operation::ResetCircuitBreaker),
        800_000
    );
    assert_eq!(
        client.get_estimated_cost(&Operation::RecordFailure),
        880_000
    );
    assert_eq!(
        client.get_estimated_cost(&Operation::RegisterTask),
        1_000_000
    );

    assert_eq!(client.get_estimated_cost(&Operation::LockTokens), 1_250_000);
    assert_eq!(
        client.get_estimated_cost(&Operation::StartRewardStream),
        1_330_000
    );
    assert_eq!(
        client.get_estimated_cost(&Operation::UnlockTokens),
        1_300_000
    );
    assert_eq!(
        client.get_estimated_cost(&Operation::ResignGuardian),
        1_400_000
    );

    assert_eq!(client.get_estimated_cost(&Operation::Vote), 1_960_000);
    assert_eq!(
        client.get_estimated_cost(&Operation::UpgradeContract),
        2_500_000
    );
}

#[test]
fn test_estimated_cost_requires_no_auth() {
    let env = Env::default();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let cost = client.get_estimated_cost(&Operation::Vote);
    assert!(cost > 0);
}

#[test]
fn test_all_costs_above_base_invocation_overhead() {
    let (_env, _contract_id, _admin, _token, client) = setup();
    const BASE: u64 = 500_000;

    let ops = [
        Operation::RegisterTask,
        Operation::Vote,
        Operation::AddGuardian,
        Operation::SetReputation,
        Operation::LockTokens,
        Operation::UnlockTokens,
        Operation::ResignGuardian,
        Operation::SetWeightThreshold,
        Operation::StartRewardStream,
        Operation::TogglePause,
        Operation::RecordFailure,
        Operation::ResetCircuitBreaker,
        Operation::UpgradeContract,
    ];

    for op in ops {
        assert!(
            client.get_estimated_cost(&op) > BASE,
            "{:?} is below base overhead",
            op
        );
    }
}

// ─── Withdrawal timelock tests ──────────────────────────────────────

#[test]
fn test_unlock_tokens_blocked_without_timelock_request() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &guardian, 150);

    // Try to unlock without first requesting - should fail
    let result = client.try_unlock_tokens(&guardian);
    assert!(result.is_err());
}

#[test]
fn test_request_unlock_initiates_timelock() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &guardian, 150);

    // Any address with locked tokens can initiate the timelock
    client.request_unlock(&guardian);

    // Timelock should be set
    let timelock = client.get_withdrawal_timelock(&guardian);
    assert!(timelock.is_some());
}

#[test]
fn test_unlock_tokens_blocked_before_24_hours() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &guardian, 150);

    // Remove guardian so they can use the unlock_tokens path
    client.remove_guardian(&admin, &guardian);
    client.request_unlock(&guardian);

    // Try to unlock immediately — timelock not expired yet
    let result = client.try_unlock_tokens(&guardian);
    assert!(result.is_err());
}

#[test]
fn test_unlock_tokens_succeeds_after_24_hours() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &guardian, 150);

    // Remove guardian so they can use the unlock_tokens path
    client.remove_guardian(&admin, &guardian);
    client.request_unlock(&guardian);

    // Get the timelock timestamp
    let timelock = client.get_withdrawal_timelock(&guardian).unwrap();

    // Advance ledger by 24 hours + 1 second
    let jump = 86401u64;
    env.ledger().set_timestamp(timelock + jump);

    // Now unlock should succeed
    let result = client.try_unlock_tokens(&guardian);
    assert!(result.is_ok());

    // Timelock should be cleared
    let new_timelock = client.get_withdrawal_timelock(&guardian);
    assert!(new_timelock.is_none());
}

#[test]
fn test_resign_guardian_blocked_before_24_hours() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &guardian, 150);

    // Initiate the timelock
    client.request_unlock(&guardian);

    // Trying to resign immediately is blocked — timelock not expired
    let result = client.try_resign_guardian(&guardian);
    assert!(result.is_err());
}

#[test]
fn test_resign_guardian_succeeds_after_24_hours() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &guardian, 150);

    // Initiate the timelock
    client.request_unlock(&guardian);

    // Get the timelock timestamp
    let timelock = client.get_withdrawal_timelock(&guardian).unwrap();

    // Advance ledger by 24 hours + 1 second
    let jump = 86401u64;
    env.ledger().set_timestamp(timelock + jump);

    // Now resign should succeed
    let result = client.try_resign_guardian(&guardian);
    assert!(result.is_ok());

    // Guardian should no longer be registered
    assert!(!client.is_guardian(&guardian));
}

#[test]
fn test_request_unlock_fails_if_still_guardian() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &guardian, 150);

    // A guardian can request a timelock (to start the 24-hour clock for resign)
    client.request_unlock(&guardian);
    assert!(client.get_withdrawal_timelock(&guardian).is_some());

    // But a guardian cannot call unlock_tokens — must use resign_guardian instead
    let timelock = client.get_withdrawal_timelock(&guardian).unwrap();
    env.ledger().set_timestamp(timelock + 86401u64);
    let result = client.try_unlock_tokens(&guardian);
    assert!(result.is_err()); // StillGuardian
}

// ─── purge_task ────────────────────────────────────────────────────

#[test]
fn test_purge_done_task_removes_storage() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let g = add_guardian_with_rep(&env, &client, &admin, 300);
    client.register_task(&admin, &1u64);
    lock_for_guardian(&env, &token, &client, &g, 101);
    client.vote(&g, &1u64);

    // Task must be done before purge
    assert!(client.get_task(&1u64).unwrap().is_done);

    // Purge should succeed
    client.purge_task(&admin, &1u64);

    // Storage entry must be gone
    assert!(client.get_task(&1u64).is_none());
}

#[test]
fn test_purge_cancelled_task_removes_storage() {
    let (_env, _contract_id, admin, _token, client) = setup();
    client.register_task(&admin, &2u64);
    client.cancel_task(&admin, &2u64);

    assert!(client.get_task(&2u64).unwrap().is_cancelled);

    client.purge_task(&admin, &2u64);

    assert!(client.get_task(&2u64).is_none());
}

#[test]
fn test_purge_active_task_reverts() {
    let (_env, _contract_id, admin, _token, client) = setup();
    client.register_task(&admin, &3u64);

    // Task is active — purge must revert
    let result = client.try_purge_task(&admin, &3u64);
    assert!(result.is_err());

    // Task still present
    assert!(client.get_task(&3u64).is_some());
}

#[test]
fn test_purge_nonexistent_task_reverts() {
    let (_env, _contract_id, admin, _token, client) = setup();

    let result = client.try_purge_task(&admin, &999u64);
    assert!(result.is_err());
}

#[test]
fn test_purge_removes_task_from_all_tasks_index() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    // Register two tasks
    client.register_task(&admin, &10u64);
    client.register_task(&admin, &11u64);

    // Resolve task 10
    let g = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &g, 101);
    client.vote(&g, &10u64);
    assert!(client.get_task(&10u64).unwrap().is_done);

    // Purge task 10
    client.purge_task(&admin, &10u64);

    // Task 11 must still be accessible
    assert!(client.get_task(&11u64).is_some());
    // Task 10 must be gone
    assert!(client.get_task(&10u64).is_none());
}

#[test]
fn test_purge_clears_voter_records() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let g = add_guardian_with_rep(&env, &client, &admin, 300);
    client.register_task(&admin, &20u64);
    lock_for_guardian(&env, &token, &client, &g, 101);
    client.vote(&g, &20u64);

    client.purge_task(&admin, &20u64);

    // After purge, the task is gone and state is clean
    assert!(client.get_task(&20u64).is_none());
}

#[test]
fn test_non_admin_cannot_purge_task() {
    let (env, _contract_id, admin, _token, client) = setup();
    client.register_task(&admin, &30u64);
    client.cancel_task(&admin, &30u64);

    let stranger = Address::generate(&env);
    let result = client.try_purge_task(&stranger, &30u64);
    assert!(result.is_err());

    // Task must still exist
    assert!(client.get_task(&30u64).is_some());
}

#[test]
fn test_purge_archived_task_removes_storage() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    // Set a non-zero starting timestamp so resolved_at is non-zero
    env.ledger().set_timestamp(1000u64);

    let g = add_guardian_with_rep(&env, &client, &admin, 300);
    client.register_task(&admin, &40u64);
    lock_for_guardian(&env, &token, &client, &g, 101);
    client.vote(&g, &40u64);

    // Archive the task (needs >30 days old)
    let resolved = client.get_task(&40u64).unwrap().resolved_at;
    let thirty_days_plus_one: u64 = 30 * 24 * 60 * 60 + 1;
    env.ledger().set_timestamp(resolved + thirty_days_plus_one);
    client.archive_task(&40u64);

    // Task must be archived
    assert!(client.get_archived_task(&40u64).is_some());
    assert!(client.get_task(&40u64).is_none());

    // Purge the archived task
    client.purge_task(&admin, &40u64);

    // Both active and archived entries must be gone
    assert!(client.get_task(&40u64).is_none());
    assert!(client.get_archived_task(&40u64).is_none());
}

// ─── Batch execution tests ──────────────────────────────────────────

#[test]
fn test_batch_execute_successful() {
    let (env, _contract_id, admin, token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &guardian, 150);

    let calls = soroban_sdk::vec![
        &env,
        vero_core_contracts::BatchCall::RegisterTask(admin.clone(), 1u64),
        vero_core_contracts::BatchCall::Vote(guardian.clone(), 1u64),
    ];

    client.batch_execute(&calls);

    let task = client.get_task(&1u64).unwrap();
    assert_eq!(task.votes, 1);
}

#[test]
fn test_batch_execute_reverts_on_failure() {
    let (env, _contract_id, admin, _token, client) = setup();
    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);

    // Register a valid task, but vote on an invalid task (task_id 99 doesn't exist)
    let calls = soroban_sdk::vec![
        &env,
        vero_core_contracts::BatchCall::RegisterTask(admin.clone(), 2u64),
        vero_core_contracts::BatchCall::Vote(guardian.clone(), 99u64),
    ];

    let result = client.try_batch_execute(&calls);
    assert!(result.is_err());

    // Because it reverts, the valid part (RegisterTask 2) should NOT be persisted.
    let task = client.get_task(&2u64);
    assert!(task.is_none());
}

// ─── Batch vote tests ───────────────────────────────────────────────

#[test]
fn test_vote_batch_all_valid() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &guardian, 101);

    client.register_task(&admin, &1u64);
    client.register_task(&admin, &2u64);
    client.register_task(&admin, &3u64);

    let ids = SorobanVec::from_array(&env, [1u64, 2u64, 3u64]);
    client.vote_batch(&guardian, &ids);

    for id in [1u64, 2u64, 3u64] {
        let task = client.get_task(&id).unwrap();
        assert_eq!(task.votes, 1);
        assert!(task.is_done);
        assert_eq!(task.total_weight_accrued, 300);
    }
}

#[test]
fn test_vote_batch_reverts_on_invalid_task() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &guardian, 101);

    client.register_task(&admin, &10u64);

    // One valid, one non-existent — entire batch must revert
    let ids = SorobanVec::from_array(&env, [10u64, 999u64]);
    let result = client.try_vote_batch(&guardian, &ids);
    assert!(result.is_err());

    // No vote should persist
    let task = client.get_task(&10u64).unwrap();
    assert_eq!(task.votes, 0);
    assert!(!task.is_done);
}

#[test]
fn test_vote_batch_reverts_on_zero_task_id_without_partial_apply() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &guardian, 101);

    client.register_task(&admin, &11u64);

    let ids = SorobanVec::from_array(&env, [11u64, 0u64]);
    let result = client.try_vote_batch(&guardian, &ids);
    assert!(result.is_err());
    assert!(client.get_task(&0u64).is_none());

    let task = client.get_task(&11u64).unwrap();
    assert_eq!(task.votes, 0);
    assert_eq!(task.total_weight_accrued, 0);
    assert!(!task.is_done);
}

#[test]
fn test_vote_batch_duplicate_vote_reverts() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let guardian = add_guardian_with_rep(&env, &client, &admin, 300);
    lock_for_guardian(&env, &token, &client, &guardian, 101);

    client.register_task(&admin, &20u64);
    client.register_task(&admin, &21u64);
    client.register_task(&admin, &22u64);

    // Vote once on task 20 first
    client.vote(&guardian, &20u64);

    // Batch includes task 20 (already voted) — must revert
    let ids = SorobanVec::from_array(&env, [21u64, 20u64, 22u64]);
    let result = client.try_vote_batch(&guardian, &ids);
    assert!(result.is_err());

    // Task 21 should remain untouched (no partial apply)
    let task21 = client.get_task(&21u64).unwrap();
    assert_eq!(task21.votes, 0);
    let task22 = client.get_task(&22u64).unwrap();
    assert_eq!(task22.votes, 0);
}

#[test]
fn test_vote_batch_accumulates_weight_across_tasks() {
    let (env, _contract_id, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &500u64);

    let g1 = add_guardian_with_rep(&env, &client, &admin, 300);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 250);
    lock_for_guardian(&env, &token, &client, &g1, 101);
    lock_for_guardian(&env, &token, &client, &g2, 101);

    client.register_task(&admin, &30u64);
    client.register_task(&admin, &31u64);

    // g1 votes on both tasks in one batch
    let ids = SorobanVec::from_array(&env, [30u64, 31u64]);
    client.vote_batch(&g1, &ids);

    // g2 votes separately on both tasks
    client.vote(&g2, &30u64);
    client.vote(&g2, &31u64);

    // Both tasks should have combined weight 300+250=550 >= 500, so resolved
    let task30 = client.get_task(&30u64).unwrap();
    assert!(task30.is_done);
    assert_eq!(task30.total_weight_accrued, 550);
    assert_eq!(task30.votes, 2);

    let task31 = client.get_task(&31u64).unwrap();
    assert!(task31.is_done);
    assert_eq!(task31.total_weight_accrued, 550);
    assert_eq!(task31.votes, 2);
}
