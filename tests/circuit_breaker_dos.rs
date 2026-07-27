//! # Circuit-breaker DoS regression tests
//!
//! These tests pin down the trust model documented in `src/circuit_breaker.rs`:
//! `record_failure` stays open to any observer, but reports are authenticated,
//! rate-limited and quota-capped per address, and the breaker only trips once
//! several independent reporters agree.
//!
//! The headline acceptance-criteria test is
//! [`test_single_address_cannot_unilaterally_pause_contract`].

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};
use vero_core_contracts::{ContractError, Role, VeroContractClient};

const LOCK_THRESHOLD: i128 = 100;

/// Mirrors the on-chain constants in `src/circuit_breaker.rs`.
const FAILURE_THRESHOLD: u32 = 50;
const REPORT_COOLDOWN_LEDGERS: u32 = 10;
const MAX_REPORTS_PER_REPORTER: u32 = 5;
const MIN_DISTINCT_REPORTERS: u32 = 3;

fn setup() -> (Env, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);

    client.initialize(&admin, &token.address(), &LOCK_THRESHOLD);
    client.grant_role(&admin, &admin, &Role::EmergencyManager);
    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.grant_role(&admin, &admin, &Role::TaskManager);

    (env, admin, client)
}

/// Advance the ledger past the per-reporter cooldown window.
fn advance_past_cooldown(env: &Env) {
    let seq = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq + REPORT_COOLDOWN_LEDGERS + 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// ACCEPTANCE CRITERIA — a single address cannot unilaterally pause the contract
// ─────────────────────────────────────────────────────────────────────────────

/// The core regression test for the reported DoS.
///
/// Before the fix, this loop of 51 `record_failure()` calls from one address
/// paused the whole contract. Now the attacker is stopped by the per-address
/// quota, the breaker never trips, and every normal operation still works.
#[test]
fn test_single_address_cannot_unilaterally_pause_contract() {
    let (env, admin, client) = setup();
    let attacker = Address::generate(&env);

    let mut accepted = 0u32;
    let mut rejected = 0u32;

    // The attacker tries well beyond the threshold, patiently waiting out the
    // cooldown between every single attempt (the strongest possible attack:
    // one report per ledger, forever).
    for _ in 0..(FAILURE_THRESHOLD + 10) {
        env.budget().reset_unlimited();
        advance_past_cooldown(&env);
        match client.try_record_failure(&attacker) {
            Ok(_) => accepted += 1,
            Err(_) => rejected += 1,
        }
    }
    env.budget().reset_unlimited();

    // Only the per-window quota was ever accepted.
    assert_eq!(
        accepted, MAX_REPORTS_PER_REPORTER,
        "one address must never contribute more than its quota"
    );
    assert!(rejected > 0, "excess reports must be rejected");
    assert_eq!(client.get_failure_count(), MAX_REPORTS_PER_REPORTER);
    assert_eq!(
        client.get_reporter_failure_count(&attacker),
        MAX_REPORTS_PER_REPORTER
    );

    // THE CONTRACT IS NOT PAUSED.
    assert!(
        !client.is_paused(),
        "a single address must not be able to pause the contract"
    );

    // …and every operation the DoS used to block still works.
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);
    assert!(client.is_guardian(&guardian));
    client.register_task(&admin, &1u64, &1u32);
    assert!(client.get_task(&1u64).is_some());
}

/// The original attack shape: a tight loop inside one transaction / one ledger.
/// The cooldown makes every call after the first fail immediately.
#[test]
fn test_loop_in_single_ledger_is_rate_limited() {
    let (env, _admin, client) = setup();
    let attacker = Address::generate(&env);

    // First report in this ledger succeeds.
    client.record_failure(&attacker);

    // The ledger sequence does not advance inside a transaction, so all
    // subsequent attempts are rejected with ReportRateLimited.
    for _ in 0..60 {
        env.budget().reset_unlimited();
        let err = client
            .try_record_failure(&attacker)
            .expect_err("second report in same ledger must be rejected");
        assert_eq!(err.unwrap(), ContractError::ReportRateLimited);
    }

    assert_eq!(client.get_failure_count(), 1);
    assert!(!client.is_paused());
}

/// Even a coalition smaller than the distinct-reporter quorum cannot trip the
/// breaker, because `MAX_REPORTS_PER_REPORTER * (MIN_DISTINCT_REPORTERS - 1)`
/// is below `FAILURE_THRESHOLD`.
#[test]
fn test_sub_quorum_coalition_cannot_pause_contract() {
    let (env, _admin, client) = setup();

    let coalition: std::vec::Vec<Address> = (0..(MIN_DISTINCT_REPORTERS - 1))
        .map(|_| Address::generate(&env))
        .collect();

    for _ in 0..(MAX_REPORTS_PER_REPORTER * 4) {
        env.budget().reset_unlimited();
        advance_past_cooldown(&env);
        for a in &coalition {
            let _ = client.try_record_failure(a);
        }
    }
    env.budget().reset_unlimited();

    assert!(
        client.get_failure_count() <= MAX_REPORTS_PER_REPORTER * (MIN_DISTINCT_REPORTERS - 1)
    );
    assert!(client.get_failure_count() < FAILURE_THRESHOLD);
    assert!(
        !client.is_paused(),
        "a sub-quorum coalition must not be able to pause the contract"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Rate limiting / quota mechanics
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_report_accepted_after_cooldown_elapses() {
    let (env, _admin, client) = setup();
    let monitor = Address::generate(&env);

    client.record_failure(&monitor);
    assert_eq!(client.get_failure_count(), 1);

    // One ledger short of the cooldown — still rejected.
    let seq = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq + REPORT_COOLDOWN_LEDGERS - 1);
    let err = client.try_record_failure(&monitor).unwrap_err();
    assert_eq!(err.unwrap(), ContractError::ReportRateLimited);

    // Cooldown elapsed — accepted.
    advance_past_cooldown(&env);
    client.record_failure(&monitor);
    assert_eq!(client.get_failure_count(), 2);
}

#[test]
fn test_reporter_quota_is_enforced() {
    let (env, _admin, client) = setup();
    let monitor = Address::generate(&env);

    for i in 0..MAX_REPORTS_PER_REPORTER {
        advance_past_cooldown(&env);
        client.record_failure(&monitor);
        assert_eq!(client.get_reporter_failure_count(&monitor), i + 1);
    }

    advance_past_cooldown(&env);
    let err = client.try_record_failure(&monitor).unwrap_err();
    assert_eq!(err.unwrap(), ContractError::ReporterQuotaExceeded);
    assert_eq!(
        client.get_reporter_failure_count(&monitor),
        MAX_REPORTS_PER_REPORTER
    );
}

#[test]
fn test_distinct_reporters_are_tracked() {
    let (env, _admin, client) = setup();
    let a = Address::generate(&env);
    let b = Address::generate(&env);

    client.record_failure(&a);
    client.record_failure(&b);

    let reporters = client.get_failure_reporters();
    assert_eq!(reporters.len(), 2);
    assert!(reporters.contains(&a));
    assert!(reporters.contains(&b));

    // Re-reporting does not duplicate the index entry.
    advance_past_cooldown(&env);
    client.record_failure(&a);
    assert_eq!(client.get_failure_reporters().len(), 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// The breaker still works for its intended purpose
// ─────────────────────────────────────────────────────────────────────────────

/// Enough independent monitors reporting genuine failures still trips the
/// breaker — the mitigation hardens the mechanism, it does not disable it.
#[test]
fn test_quorum_of_independent_reporters_still_trips_breaker() {
    let (env, _admin, client) = setup();

    // 11 monitors × 5 reports each = 55 > 50 threshold, with 11 >= 3 distinct.
    let monitors: std::vec::Vec<Address> = (0..11).map(|_| Address::generate(&env)).collect();

    'outer: for _ in 0..MAX_REPORTS_PER_REPORTER {
        env.budget().reset_unlimited();
        advance_past_cooldown(&env);
        for m in &monitors {
            let _ = client.try_record_failure(m);
            if client.is_paused() {
                break 'outer;
            }
        }
    }

    assert!(
        client.is_paused(),
        "a quorum of independent reporters must still be able to trip the breaker"
    );
    assert!(client.get_failure_count() > FAILURE_THRESHOLD);
}

/// The breaker must not trip on count alone when too few distinct reporters
/// contributed — count AND quorum are both required.
#[test]
fn test_breaker_requires_distinct_reporter_quorum() {
    let (env, _admin, client) = setup();

    // Two reporters can at most contribute 10 reports total — far below the
    // threshold — so the quorum rule is never even reached.
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    for _ in 0..MAX_REPORTS_PER_REPORTER {
        advance_past_cooldown(&env);
        let _ = client.try_record_failure(&a);
        let _ = client.try_record_failure(&b);
    }

    assert_eq!(client.get_failure_reporters().len(), 2);
    assert!(client.get_failure_reporters().len() < MIN_DISTINCT_REPORTERS);
    assert!(!client.is_paused());
}

// ─────────────────────────────────────────────────────────────────────────────
// Reset semantics
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_reset_clears_per_reporter_accounting() {
    let (env, admin, client) = setup();
    let monitor = Address::generate(&env);

    for _ in 0..MAX_REPORTS_PER_REPORTER {
        advance_past_cooldown(&env);
        client.record_failure(&monitor);
    }
    assert_eq!(
        client.get_reporter_failure_count(&monitor),
        MAX_REPORTS_PER_REPORTER
    );

    client.reset_circuit_breaker(&admin);

    assert_eq!(client.get_failure_count(), 0);
    assert_eq!(client.get_reporter_failure_count(&monitor), 0);
    assert_eq!(client.get_failure_reporters().len(), 0);
    assert!(!client.is_paused());

    // A fresh window: the monitor may report again.
    client.record_failure(&monitor);
    assert_eq!(client.get_failure_count(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Trusted-reporters-only escape hatch
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_trusted_reporters_only_mode_blocks_strangers() {
    let (env, admin, client) = setup();
    let stranger = Address::generate(&env);
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);

    assert!(!client.is_trusted_reporters_only());
    client.set_trusted_reporters_only(&admin, &true);
    assert!(client.is_trusted_reporters_only());

    let err = client.try_record_failure(&stranger).unwrap_err();
    assert_eq!(err.unwrap(), ContractError::UnauthorizedReporter);
    assert_eq!(client.get_failure_count(), 0);

    // Guardians and EmergencyManagers are trusted.
    client.record_failure(&guardian);
    assert_eq!(client.get_failure_count(), 1);
    client.record_failure(&admin);
    assert_eq!(client.get_failure_count(), 2);

    // Re-opening restores permissionless (but still rate-limited) reporting.
    client.set_trusted_reporters_only(&admin, &false);
    client.record_failure(&stranger);
    assert_eq!(client.get_failure_count(), 3);
}

#[test]
fn test_only_emergency_manager_can_toggle_trusted_mode() {
    let (env, _admin, client) = setup();
    let stranger = Address::generate(&env);

    let err = client
        .try_set_trusted_reporters_only(&stranger, &true)
        .unwrap_err();
    assert_eq!(err.unwrap(), ContractError::NotAuthorized);
    assert!(!client.is_trusted_reporters_only());
}

// ─────────────────────────────────────────────────────────────────────────────
// Reporter authentication
// ─────────────────────────────────────────────────────────────────────────────

/// `record_failure` now requires the reporter's signature, so reports are
/// attributable and cannot be spoofed on another address's behalf.
#[test]
fn test_record_failure_requires_reporter_auth() {
    let env = Env::default();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);

    env.mock_all_auths();
    client.initialize(&admin, &token.address(), &LOCK_THRESHOLD);

    // Without mocked auth the call must fail the signature check.
    env.set_auths(&[]);
    let reporter = Address::generate(&env);
    assert!(
        client.try_record_failure(&reporter).is_err(),
        "unauthenticated report must be rejected"
    );

    env.mock_all_auths();
    assert_eq!(client.get_failure_count(), 0);
}

/// The zero address cannot be used as a reporter identity.
#[test]
fn test_zero_address_cannot_report() {
    let (env, _admin, client) = setup();
    let zero = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ));

    let err = client.try_record_failure(&zero).unwrap_err();
    assert_eq!(err.unwrap(), ContractError::InvalidAddress);
    assert_eq!(client.get_failure_count(), 0);
}
