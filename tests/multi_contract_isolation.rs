//! # Multi-Contract State Isolation Integration Test
//!
//! This test deploys two independent `VeroContract` instances within the same
//! Soroban test environment and verifies that their instance storage is fully
//! isolated — guardian state, reputation scores, and task records registered
//! on one instance are invisible to the other.
//!
//! ## Why this matters
//!
//! All contract state lives in **instance storage** scoped to the contract
//! deployment address (per `README.md`'s Storage Design section). While
//! instance-storage isolation is a Soroban platform guarantee, a future
//! refactor that accidentally uses `persistent` or `temporary` storage with
//! a global key would silently break that guarantee. This test exercises the
//! boundary so such regressions surface immediately in CI.
//!
//! ## What is tested (acceptance criteria from issue #169)
//!
//! | Dimension       | Contract A action              | Contract B assertion           |
//! |-----------------|--------------------------------|--------------------------------|
//! | Guardian state  | `add_guardian(guardian_a)`     | `is_guardian(guardian_a)` → false |
//! | Reputation      | `set_reputation(guardian_a, 500)` | `get_reputation(guardian_a)` → None |
//! | Task state      | `register_task(task_id=1)`     | `get_task(1)` → None           |
//! | Reverse check   | *(nothing added to B)*         | B's own guardian/task absent from A |

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use vero_core_contracts::{Role, VeroContract, VeroContractClient};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Deploy a fresh `VeroContract` instance, initialize it with the given admin
/// and token, grant `GuardianManager` and `TaskManager` to the admin, and
/// return a ready-to-use client.
fn deploy_instance<'a>(
    env: &'a Env,
    admin: &Address,
    token_addr: &Address,
    lock_threshold: i128,
) -> VeroContractClient<'a> {
    let contract_id = env.register_contract(None, VeroContract);
    let client = VeroContractClient::new(env, &contract_id);

    client.initialize(admin, token_addr, &lock_threshold);
    client.grant_role(admin, admin, &Role::GuardianManager);
    client.grant_role(admin, admin, &Role::TaskManager);

    client
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// Deploy two contract instances backed by the same token (realistic: they
/// could share a token contract), configure one with a guardian and a task,
/// and assert the other knows nothing about either.
#[test]
fn test_guardian_state_does_not_leak_between_instances() {
    let env = Env::default();
    env.mock_all_auths();

    // Two independent admins, one shared token.
    let admin_a = Address::generate(&env);
    let admin_b = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    let client_a = deploy_instance(&env, &admin_a, &token_addr, 1);
    let client_b = deploy_instance(&env, &admin_b, &token_addr, 1);

    // ── Populate contract A ──────────────────────────────────────────────────
    let guardian_a = Address::generate(&env);
    client_a.add_guardian(&admin_a, &guardian_a);
    client_a.set_reputation(&admin_a, &guardian_a, &500u64);

    let task_id_a: u64 = 1;
    client_a.register_task(&admin_a, &task_id_a, &1u32);

    // ── Contract A — sanity-check that A's own state is readable ────────────
    assert!(
        client_a.is_guardian(&guardian_a),
        "A: guardian_a should be registered in instance A"
    );
    assert_eq!(
        client_a.get_reputation(&guardian_a),
        Some(500),
        "A: reputation for guardian_a should be 500 in instance A"
    );
    assert!(
        client_a.get_task(&task_id_a).is_some(),
        "A: task 1 should exist in instance A"
    );

    // ── Contract B — assert complete isolation ───────────────────────────────
    assert!(
        !client_b.is_guardian(&guardian_a),
        "B: guardian_a must NOT appear as a guardian in instance B (storage leak!)"
    );
    assert_eq!(
        client_b.get_reputation(&guardian_a),
        None,
        "B: reputation for guardian_a must be None in instance B (storage leak!)"
    );
    assert!(
        client_b.get_task(&task_id_a).is_none(),
        "B: task 1 must NOT exist in instance B (storage leak!)"
    );
}

/// Mirror of the previous test: populate B and verify A is unaffected.
/// Together the two tests confirm bidirectional isolation.
#[test]
fn test_state_registered_in_b_does_not_appear_in_a() {
    let env = Env::default();
    env.mock_all_auths();

    let admin_a = Address::generate(&env);
    let admin_b = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    let client_a = deploy_instance(&env, &admin_a, &token_addr, 1);
    let client_b = deploy_instance(&env, &admin_b, &token_addr, 1);

    // ── Populate contract B ──────────────────────────────────────────────────
    let guardian_b = Address::generate(&env);
    client_b.add_guardian(&admin_b, &guardian_b);
    client_b.set_reputation(&admin_b, &guardian_b, &999u64);

    let task_id_b: u64 = 42;
    client_b.register_task(&admin_b, &task_id_b, &1u32);

    // ── Contract B — sanity-check ────────────────────────────────────────────
    assert!(client_b.is_guardian(&guardian_b));
    assert_eq!(client_b.get_reputation(&guardian_b), Some(999));
    assert!(client_b.get_task(&task_id_b).is_some());

    // ── Contract A — must have no knowledge of B's state ────────────────────
    assert!(
        !client_a.is_guardian(&guardian_b),
        "A: guardian_b must NOT appear in instance A (storage leak!)"
    );
    assert_eq!(
        client_a.get_reputation(&guardian_b),
        None,
        "A: reputation for guardian_b must be None in instance A (storage leak!)"
    );
    assert!(
        client_a.get_task(&task_id_b).is_none(),
        "A: task 42 must NOT exist in instance A (storage leak!)"
    );
}

/// Verify that each instance maintains an independent admin: the admin of A
/// cannot be read back as the admin of B, confirming that even initialisation
/// state is fully scoped to each deployment.
#[test]
fn test_admin_state_is_isolated_per_instance() {
    let env = Env::default();
    env.mock_all_auths();

    let admin_a = Address::generate(&env);
    let admin_b = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    let client_a = deploy_instance(&env, &admin_a, &token_addr, 1);
    let client_b = deploy_instance(&env, &admin_b, &token_addr, 1);

    // Each instance stores its own admin.
    assert_eq!(
        client_a.get_admin(),
        Some(admin_a.clone()),
        "A: get_admin should return admin_a"
    );
    assert_eq!(
        client_b.get_admin(),
        Some(admin_b.clone()),
        "B: get_admin should return admin_b"
    );

    // Cross-check: A's admin is not B's admin and vice-versa.
    assert_ne!(
        client_a.get_admin(),
        client_b.get_admin(),
        "admin addresses must differ between the two instances"
    );
}

/// Full end-to-end isolation: both instances are populated with overlapping
/// task IDs and guardian addresses, and we assert neither can see the other's
/// entries.
#[test]
fn test_overlapping_task_ids_are_isolated_per_instance() {
    let env = Env::default();
    env.mock_all_auths();

    let admin_a = Address::generate(&env);
    let admin_b = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    let client_a = deploy_instance(&env, &admin_a, &token_addr, 1);
    let client_b = deploy_instance(&env, &admin_b, &token_addr, 1);

    // Both instances register the SAME task id (100) independently.
    let shared_task_id: u64 = 100;
    client_a.register_task(&admin_a, &shared_task_id, &1u32);
    client_b.register_task(&admin_b, &shared_task_id, &2u32);

    // Task exists in both, but with the min_votes_required each was given.
    let task_in_a = client_a.get_task(&shared_task_id).unwrap();
    let task_in_b = client_b.get_task(&shared_task_id).unwrap();

    assert_eq!(task_in_a.min_votes_required, 1, "A: min_votes should be 1");
    assert_eq!(task_in_b.min_votes_required, 2, "B: min_votes should be 2");

    // The records are independent — same key, different storage namespaces.
    assert_ne!(
        task_in_a.min_votes_required, task_in_b.min_votes_required,
        "tasks with the same id must hold independent state in each instance"
    );

    // Both instances register the SAME guardian address with different reputations.
    let shared_guardian = Address::generate(&env);
    client_a.add_guardian(&admin_a, &shared_guardian);
    client_b.add_guardian(&admin_b, &shared_guardian);

    client_a.set_reputation(&admin_a, &shared_guardian, &100u64);
    client_b.set_reputation(&admin_b, &shared_guardian, &200u64);

    assert_eq!(
        client_a.get_reputation(&shared_guardian),
        Some(100),
        "A: shared_guardian reputation must be 100 in instance A"
    );
    assert_eq!(
        client_b.get_reputation(&shared_guardian),
        Some(200),
        "B: shared_guardian reputation must be 200 in instance B"
    );
}

/// Verify that pause / circuit-breaker state is also isolated: pausing instance
/// A must not affect the operational status of instance B.
#[test]
fn test_pause_state_is_isolated_per_instance() {
    let env = Env::default();
    env.mock_all_auths();

    let admin_a = Address::generate(&env);
    let admin_b = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    let client_a = deploy_instance(&env, &admin_a, &token_addr, 1);
    let client_b = deploy_instance(&env, &admin_b, &token_addr, 1);

    // Grant EmergencyManager role so admin_a can pause instance A.
    client_a.grant_role(&admin_a, &admin_a, &Role::EmergencyManager);

    // Both instances start unpaused.
    assert!(!client_a.is_paused(), "A: should not be paused initially");
    assert!(!client_b.is_paused(), "B: should not be paused initially");

    // Pause only instance A.
    client_a.pause(&admin_a);

    // Instance A is paused; instance B must remain operational.
    assert!(client_a.is_paused(), "A: must be paused after pause()");
    assert!(
        !client_b.is_paused(),
        "B: must NOT be paused — pause on A must not leak into B (storage leak!)"
    );
}
