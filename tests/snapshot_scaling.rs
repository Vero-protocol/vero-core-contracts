//! Regression tests for GitHub issue #143: `get_snapshot`/`record_snapshot`
//! cost scaled unbounded with total guardians/tasks/reward streams.
//!
//! These tests document the instruction-cost growth curve of the atomic
//! `get_snapshot` as guardian/task counts increase, and prove the two-part
//! mitigation:
//!   1. `get_snapshot`/`record_snapshot` refuse to run (return
//!      `SnapshotTooLarge`) once any collection exceeds
//!      `MAX_SNAPSHOT_COLLECTION_SIZE`, instead of silently growing until the
//!      ledger's per-transaction CPU budget is exceeded.
//!   2. The paginated API (`get_snapshot_meta` + `get_guardians_page` +
//!      `get_tasks_page`) reads at most `O(limit)` entries per call — not
//!      `O(total collection size)` — so it stays cheaply, predictably
//!      invokable at scales (1000 guardians/tasks) where `get_snapshot` can
//!      no longer run at all. Its absolute cost still has a mild dependency
//!      on total instance-storage footprint (an inherent property of
//!      Soroban's shared-instance-ledger-entry storage model, not something
//!      pagination alone can fully eliminate) — see the test comments below
//!      for detail.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use vero_core_contracts::{ContractError, Role, VeroContractClient};

const LOCK_THRESHOLD: i128 = 0;

fn setup() -> (Env, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    // The test `Env`'s budget accumulates across every contract call made
    // against it, not just the one under measurement. Run setup (which can
    // involve hundreds of calls to build up large guardian/task counts)
    // unmetered, then each test resets to a fresh metered budget right
    // before the single call it wants to measure.
    env.budget().reset_unlimited();

    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);

    client.initialize(&admin, &token.address(), &LOCK_THRESHOLD);
    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.grant_role(&admin, &admin, &Role::TaskManager);

    (env, admin, client)
}

fn add_guardians(env: &Env, client: &VeroContractClient, admin: &Address, count: u32) {
    for _ in 0..count {
        let g = Address::generate(env);
        client.add_guardian(admin, &g);
        client.set_reputation(admin, &g, &100u64);
    }
}

fn add_tasks(client: &VeroContractClient, admin: &Address, count: u32, start_id: u64) {
    for i in 0..count {
        client.register_task(admin, &(start_id + i as u64), &1u32);
    }
}

/// Measures `get_snapshot`'s CPU instruction cost at increasing guardian
/// counts (all below `MAX_SNAPSHOT_COLLECTION_SIZE`) and asserts the growth
/// is monotonic — i.e. cost scales with collection size, as expected for the
/// atomic (non-paginated) path. This documents the growth curve referenced
/// in issue #143's acceptance criteria.
#[test]
fn get_snapshot_cost_grows_with_guardian_count() {
    let sizes = [10u32, 100u32];
    let mut costs = Vec::new();

    for size in sizes {
        let (env, admin, client) = setup();
        add_guardians(&env, &client, &admin, size);

        // `reset_tracker()` alone does NOT zero the CPU instruction counter
        // (it only resets the calibration cost-tracker map) — only
        // `reset_unlimited()`/`reset_limits()`/`reset_default()` do that, via
        // `cpu_insns.reset(..)`. Using `reset_tracker()` here would silently
        // fold all of `setup()`'s cost into the measurement below.
        env.budget().reset_unlimited();
        let snapshot = client.get_snapshot();
        let cpu = env.budget().cpu_instruction_cost();

        assert_eq!(snapshot.guardians.len(), size);
        costs.push((size, cpu));
    }

    println!("get_snapshot cost by guardian count: {:?}", costs);

    // Cost must strictly increase as the tracked collection grows — this is
    // exactly the unbounded-scaling behavior issue #143 flags. The mitigation
    // is not "make this flat" (impossible for an atomic full-state read) but
    // "cap it and provide a bounded-cost alternative", proven below.
    assert!(
        costs[1].1 > costs[0].1,
        "expected get_snapshot cost to grow with guardian count: {:?}",
        costs
    );
}

/// Same growth-curve measurement, but driven by task count instead of
/// guardian count, since `get_snapshot` also iterates `AllTasks` (tasks +
/// per-task voter lists).
#[test]
fn get_snapshot_cost_grows_with_task_count() {
    let sizes = [10u32, 100u32];
    let mut costs = Vec::new();

    for size in sizes {
        let (env, admin, client) = setup();
        add_tasks(&client, &admin, size, 1);

        // `reset_tracker()` alone does NOT zero the CPU instruction counter
        // (it only resets the calibration cost-tracker map) — only
        // `reset_unlimited()`/`reset_limits()`/`reset_default()` do that, via
        // `cpu_insns.reset(..)`. Using `reset_tracker()` here would silently
        // fold all of `setup()`'s cost into the measurement below.
        env.budget().reset_unlimited();
        let snapshot = client.get_snapshot();
        let cpu = env.budget().cpu_instruction_cost();

        assert_eq!(snapshot.tasks.len(), size);
        costs.push((size, cpu));
    }

    println!("get_snapshot cost by task count: {:?}", costs);

    assert!(
        costs[1].1 > costs[0].1,
        "expected get_snapshot cost to grow with task count: {:?}",
        costs
    );
}

/// Proves the cap: once guardian count exceeds `MAX_SNAPSHOT_COLLECTION_SIZE`
/// (200), `get_snapshot`/`record_snapshot` revert deterministically with
/// `SnapshotTooLarge` instead of attempting an ever-larger read that could
/// eventually exceed the ledger's per-transaction CPU budget and become
/// permanently uninvokable.
#[test]
fn get_snapshot_rejects_oversized_guardian_collection() {
    let (env, admin, client) = setup();
    add_guardians(&env, &client, &admin, 201);

    let result = client.try_get_snapshot();
    assert!(matches!(result, Err(Ok(ContractError::SnapshotTooLarge))));

    let record_result = client.try_record_snapshot();
    assert!(matches!(
        record_result,
        Err(Ok(ContractError::SnapshotTooLarge))
    ));
}

/// Proves the paginated API stays cheaply, predictably invokable at scales
/// (1000 guardians) where `get_snapshot` refuses to run at all.
///
/// Note on what "bounded" means here: Soroban's `instance()` storage bundles
/// *every* instance key into a single ledger footprint, so touching instance
/// storage at all carries a fixed cost that mildly tracks total instance
/// storage size — no amount of pagination over `instance()` keys can make a
/// single call's cost perfectly flat. What pagination *does* guarantee is
/// that a page's cost is `O(limit)`, not `O(total collection size)`: it does
/// exactly `2 * limit` enrichment reads (guardian flag + reputation) no
/// matter how many guardians exist in total, instead of the `2 * N` reads
/// `get_snapshot` would need. That's what keeps it comfortably invokable at
/// 1000 guardians while `get_snapshot` is capped out entirely by
/// `MAX_SNAPSHOT_COLLECTION_SIZE` (200).
#[test]
fn guardians_page_cost_stays_bounded_past_the_snapshot_cap() {
    // Comfortably above what any single bounded-page call should ever need;
    // well below what an O(N) full-collection read would cost once N grows
    // large (see `get_snapshot_cost_grows_with_guardian_count`).
    const MAX_ACCEPTABLE_PAGE_COST: u64 = 100_000_000;

    let sizes = [50u32, 100u32, 1000u32];
    let mut costs = Vec::new();

    for size in sizes {
        let (env, admin, client) = setup();
        add_guardians(&env, &client, &admin, size);

        let meta = client.get_snapshot_meta();
        assert_eq!(meta.guardian_count, size);

        // `reset_tracker()` alone does NOT zero the CPU instruction counter
        // (it only resets the calibration cost-tracker map) — only
        // `reset_unlimited()`/`reset_limits()`/`reset_default()` do that, via
        // `cpu_insns.reset(..)`. Using `reset_tracker()` here would silently
        // fold all of `setup()`'s cost into the measurement below.
        env.budget().reset_unlimited();
        let page = client.get_guardians_page(&0u32, &50u32);
        let cpu = env.budget().cpu_instruction_cost();

        assert_eq!(page.len(), size.min(50));
        costs.push((size, cpu));
    }

    println!(
        "get_guardians_page(0, 50) cost by total guardian count: {:?}",
        costs
    );

    for (size, cpu) in &costs {
        assert!(
            *cpu < MAX_ACCEPTABLE_PAGE_COST,
            "get_guardians_page cost {} at total guardian count {} exceeded the bounded-page ceiling",
            cpu,
            size
        );
    }

    // At 1000 guardians, get_snapshot cannot run at all — the paginated call
    // still can, at a fraction of what a hypothetical unbounded read would cost.
    let (env, admin, client) = setup();
    add_guardians(&env, &client, &admin, 1000);
    assert!(matches!(
        client.try_get_snapshot(),
        Err(Ok(ContractError::SnapshotTooLarge))
    ));
    let page = client.get_guardians_page(&500u32, &50u32);
    assert_eq!(page.len(), 50);
}

/// Same bounded-cost proof for the paginated task API at 1000 tasks — the
/// exact scale that would make the atomic `get_snapshot` uninvokable. See
/// `guardians_page_cost_stays_bounded_past_the_snapshot_cap` for what
/// "bounded" does and doesn't mean under Soroban's shared instance-storage
/// footprint.
#[test]
fn tasks_page_cost_stays_bounded_past_the_snapshot_cap() {
    const MAX_ACCEPTABLE_PAGE_COST: u64 = 100_000_000;

    let sizes = [50u32, 100u32, 1000u32];
    let mut costs = Vec::new();

    for size in sizes {
        let (env, admin, client) = setup();
        add_tasks(&client, &admin, size, 1);

        // `reset_tracker()` alone does NOT zero the CPU instruction counter
        // (it only resets the calibration cost-tracker map) — only
        // `reset_unlimited()`/`reset_limits()`/`reset_default()` do that, via
        // `cpu_insns.reset(..)`. Using `reset_tracker()` here would silently
        // fold all of `setup()`'s cost into the measurement below.
        env.budget().reset_unlimited();
        let page = client.get_tasks_page(&0u32, &50u32);
        let cpu = env.budget().cpu_instruction_cost();

        assert_eq!(page.len(), size.min(50));
        costs.push((size, cpu));
    }

    println!(
        "get_tasks_page(0, 50) cost by total task count: {:?}",
        costs
    );

    for (size, cpu) in &costs {
        assert!(
            *cpu < MAX_ACCEPTABLE_PAGE_COST,
            "get_tasks_page cost {} at total task count {} exceeded the bounded-page ceiling",
            cpu,
            size
        );
    }

    let (_env, admin, client) = setup();
    add_tasks(&client, &admin, 1000, 1);
    assert!(matches!(
        client.try_get_snapshot(),
        Err(Ok(ContractError::SnapshotTooLarge))
    ));
    let page = client.get_tasks_page(&500u32, &50u32);
    assert_eq!(page.len(), 50);
}

/// `get_guardians_page`/`get_tasks_page` must ignore an oversized `limit`
/// request and cap it server-side, so a caller can't defeat the bounded-cost
/// guarantee by simply asking for a huge page.
#[test]
fn page_limit_is_capped_server_side() {
    let (env, admin, client) = setup();
    add_guardians(&env, &client, &admin, 80);

    let page = client.get_guardians_page(&0u32, &10_000u32);
    assert_eq!(page.len(), 50, "limit must be capped at MAX_PAGE_LIMIT");
}
