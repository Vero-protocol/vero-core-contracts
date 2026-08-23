#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use vero_core_contracts::VeroContractClient;

#[test]
fn test_registry_starts_clean() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    assert!(client.get_task(&1u64).is_none());
    assert!(client.get_reward_stream(&1u64).is_none());
    assert_eq!(client.get_weight_threshold(), 300);

    let stranger = Address::generate(&env);
    assert_eq!(client.calculate_voting_power(&stranger), None);
    assert_eq!(client.get_reputation(&stranger), None);
}

#[test]
fn test_reinitialize_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    client.initialize(&admin, &token_addr, &100i128);

    let result = client.try_initialize(&admin, &token_addr, &100i128);
    assert!(result.is_err(), "second initialize() must revert");
}

#[test]
fn test_admin_stored_on_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);

    client.initialize(&admin, &token.address(), &100i128);

    assert_eq!(client.get_admin(), Some(admin));
}

/// `initialize` must authenticate the account it installs as Admin.
///
/// Without `admin.require_auth()`, an observer can front-run initialization on
/// a deployed-but-uninitialized contract and install themselves as Admin —
/// which gates `grant_role`, `upgrade_contract`, `set_upgrade_signers` and
/// `migrate_storage`.
#[test]
#[should_panic]
fn test_initialize_requires_admin_auth() {
    let env = Env::default();
    // Deliberately no mock_all_auths(): the call must fail without admin's
    // signature. If require_auth() is ever removed, this test stops panicking.
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    client.initialize(&admin, &token, &0);
}

/// A negative lock threshold permanently disables the vote stake requirement.
///
/// `voting.rs` rejects a vote when `locked_balance <= threshold`. With a
/// threshold of -1, a guardian holding nothing evaluates `0 <= -1` as false and
/// is allowed to vote. `LockThreshold` has no setter, so this cannot be undone.
/// Zero is legitimate — it means "any non-zero stake" — so only negatives are
/// rejected.
#[test]
fn test_initialize_rejects_negative_lock_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    assert!(client.try_initialize(&admin, &token, &-1).is_err());

    // Zero remains valid: it requires a strictly positive locked balance.
    client.initialize(&admin, &token, &0);
    assert!(client.get_admin().is_some());
}
