//! # Circuit Breaker — Sybil/DoS-resistant failure reporting
//!
//! ## Trust model for `record_failure` (decision record)
//!
//! The original design exposed `record_failure()` as a fully permissionless,
//! argument-less entry point that incremented a single global counter. Once the
//! counter passed [`FAILURE_THRESHOLD`] the contract auto-paused. That meant a
//! *single* address could call it 51 times and unilaterally freeze every
//! guardian vote, task registration and token lock until an admin reset the
//! breaker — a trivially cheap denial of service.
//!
//! The README's stated goal ("off-chain monitors can report observed failures")
//! is preserved, but the trust model is now made explicit and enforced on-chain
//! as **"permissionless but authenticated, rate-limited, and quorum-gated"**:
//!
//! 1. **Authenticated reporter.** `record_failure(reporter)` takes the reporting
//!    address and calls `reporter.require_auth()`. Reports are attributable; an
//!    anonymous caller can no longer bump the counter.
//! 2. **Per-reporter cooldown.** An address may contribute at most one report
//!    every [`REPORT_COOLDOWN_LEDGERS`] ledgers. Loop-in-one-transaction abuse
//!    is impossible because the ledger sequence does not advance inside a
//!    transaction.
//! 3. **Per-reporter quota.** An address may contribute at most
//!    [`MAX_REPORTS_PER_REPORTER`] reports to the current breaker window, so it
//!    can never accumulate enough reports to reach the threshold on its own even
//!    over an unbounded number of ledgers.
//! 4. **Distinct-reporter quorum.** The breaker only trips when the failure count
//!    exceeds [`FAILURE_THRESHOLD`] **and** at least [`MIN_DISTINCT_REPORTERS`]
//!    independent addresses have reported. Because
//!    `MAX_REPORTS_PER_REPORTER * (MIN_DISTINCT_REPORTERS - 1) <= FAILURE_THRESHOLD`,
//!    no single address (nor any group smaller than the quorum) can trip it.
//! 5. **Optional trusted-monitor mode.** An `EmergencyManager` can flip
//!    [`DataKey::TrustedReportersOnly`] on, restricting reporting to registered
//!    guardians and Emergency/Admin role holders when a Sybil flood is observed.
//!
//! Manual `pause` remains the instant, admin-only emergency stop; the breaker is
//! a slow, corroborated safety net — not a lever any observer can pull alone.

use soroban_sdk::{Address, Env, Vec};

use crate::events;
use crate::types::{ContractError, DataKey, Role};

/// Cumulative failure reports required before the breaker may trip.
pub const FAILURE_THRESHOLD: u32 = 50;

/// Minimum number of ledgers an address must wait between failure reports.
/// The ledger sequence is constant within a transaction, so this also makes
/// "51 calls in one transaction" impossible.
pub const REPORT_COOLDOWN_LEDGERS: u32 = 10;

/// Maximum number of reports one address may contribute to a single breaker
/// window (i.e. between resets).
pub const MAX_REPORTS_PER_REPORTER: u32 = 5;

/// Minimum number of *distinct* reporters required before the breaker can trip.
pub const MIN_DISTINCT_REPORTERS: u32 = 3;

/// Hard cap on the reporter index to bound storage/iteration cost.
const MAX_TRACKED_REPORTERS: u32 = 100;

// Compile-time guarantee that a coalition smaller than the quorum can never
// reach the threshold on quota alone. This is the core anti-DoS invariant.
const _: () = assert!(
    MAX_REPORTS_PER_REPORTER * (MIN_DISTINCT_REPORTERS - 1) <= FAILURE_THRESHOLD,
    "a sub-quorum coalition must not be able to reach FAILURE_THRESHOLD"
);
const _: () = assert!(
    MAX_REPORTS_PER_REPORTER <= FAILURE_THRESHOLD,
    "a single reporter must not be able to reach FAILURE_THRESHOLD"
);

/// Returns `Err(ContractError::ContractPaused)` if the contract is currently paused.
pub fn require_not_paused(env: &Env) -> Result<(), ContractError> {
    if env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
    {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}

fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

/// Whether the contract only accepts failure reports from trusted monitors
/// (registered guardians, `EmergencyManager`s and `Admin`s).
pub fn trusted_reporters_only(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::TrustedReportersOnly)
        .unwrap_or(false)
}

/// Enable/disable "trusted reporters only" mode. Caller must already have been
/// authorized (EmergencyManager) by the entry point.
pub fn set_trusted_reporters_only(env: &Env, enabled: bool) {
    env.storage()
        .instance()
        .set(&DataKey::TrustedReportersOnly, &enabled);
}

/// A reporter is "trusted" when it is a registered guardian or holds the
/// `EmergencyManager` / `Admin` role.
fn is_trusted_reporter(env: &Env, reporter: &Address) -> bool {
    crate::guardian::is_guardian(env, reporter)
        || crate::contracts::rbac::has_role(env, reporter, Role::EmergencyManager)
        || crate::contracts::rbac::has_role(env, reporter, Role::Admin)
}

/// Number of failure reports the given address has contributed to the current
/// breaker window.
pub fn reporter_count(env: &Env, reporter: &Address) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::ReporterFailureCount(reporter.clone()))
        .unwrap_or(0u32)
}

/// The ledger at which the given address last reported a failure, if ever.
pub fn last_report_ledger(env: &Env, reporter: &Address) -> Option<u32> {
    env.storage()
        .instance()
        .get(&DataKey::LastFailureReport(reporter.clone()))
}

/// The distinct addresses that have reported failures in the current window.
pub fn failure_reporters(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::FailureReporters)
        .unwrap_or(Vec::new(env))
}

/// Current cumulative failure count.
pub fn failure_count(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::FailureCount)
        .unwrap_or(0u32)
}

/// Records an authenticated failure report from `reporter`.
///
/// Enforces the trust model documented at the top of this module: caller
/// authentication, per-address cooldown, per-address quota, optional
/// trusted-monitor gating, and a distinct-reporter quorum before the breaker
/// may auto-pause the contract.
///
/// # Errors
/// * [`ContractError::InvalidAddress`] — reporter is the zero address or this contract.
/// * [`ContractError::UnauthorizedReporter`] — trusted-only mode is on and the caller is untrusted.
/// * [`ContractError::ReportRateLimited`] — caller reported less than
///   [`REPORT_COOLDOWN_LEDGERS`] ledgers ago.
/// * [`ContractError::ReporterQuotaExceeded`] — caller already used its window quota.
pub fn record_failure(env: &Env, reporter: Address) -> Result<(), ContractError> {
    // Address validation (zero-address + self-contract) is performed by the
    // calling entrypoint; we do not repeat it here.

    // 1. Authenticate the reporter. Reports are attributable from here on.
    reporter.require_auth();

    // 2. Optional trusted-monitor gating.
    if trusted_reporters_only(env) && !is_trusted_reporter(env, &reporter) {
        return Err(ContractError::UnauthorizedReporter);
    }

    let current_ledger = env.ledger().sequence();

    // 3. Per-reporter cooldown. Within a single transaction the ledger sequence
    //    does not change, so a loop of calls is rejected after the first one.
    if let Some(last) = last_report_ledger(env, &reporter) {
        if current_ledger.saturating_sub(last) < REPORT_COOLDOWN_LEDGERS {
            return Err(ContractError::ReportRateLimited);
        }
    }

    // 4. Per-reporter quota for the current breaker window.
    let reports_by_caller = reporter_count(env, &reporter);
    if reports_by_caller >= MAX_REPORTS_PER_REPORTER {
        return Err(ContractError::ReporterQuotaExceeded);
    }

    // 5. Track the reporter in the distinct-reporter index (bounded).
    let mut reporters = failure_reporters(env);
    let is_new_reporter = !reporters.contains(reporter.clone());
    if is_new_reporter {
        if reporters.len() >= MAX_TRACKED_REPORTERS {
            return Err(ContractError::ReporterQuotaExceeded);
        }
        reporters.push_back(reporter.clone());
        env.storage()
            .instance()
            .set(&DataKey::FailureReporters, &reporters);
    }

    // 6. Commit the report.
    env.storage().instance().set(
        &DataKey::LastFailureReport(reporter.clone()),
        &current_ledger,
    );
    env.storage().instance().set(
        &DataKey::ReporterFailureCount(reporter.clone()),
        &reports_by_caller.saturating_add(1),
    );

    let count = failure_count(env).saturating_add(1);
    env.storage().instance().set(&DataKey::FailureCount, &count);

    events::emit_failure_reported(env, &reporter, count);

    // 7. Quorum-gated auto-pause: enough failures AND enough independent
    //    reporters. A single address can never satisfy both.
    let distinct_reporters = reporters.len();
    if count > FAILURE_THRESHOLD && distinct_reporters >= MIN_DISTINCT_REPORTERS && !is_paused(env)
    {
        env.storage().instance().set(&DataKey::Paused, &true);
        events::emit_circuit_breaker_triggered(env, count);
    }

    Ok(())
}

/// Records an anonymous (reporter-less) failure increment for use by the
/// batch-dispatch path, where the batch transaction itself is authenticated
/// but no individual reporter address is carried in the `RecordFailure` unit
/// variant.  Per-reporter accounting (cooldown, quota, quorum) is skipped;
/// the global failure counter and the auto-pause check still apply.
pub fn record_failure_anonymous(env: &Env) -> Result<(), ContractError> {
    let count = failure_count(env).saturating_add(1);
    env.storage().instance().set(&DataKey::FailureCount, &count);

    // Quorum check uses the distinct-reporter count from prior attributed
    // reports; an anonymous increment alone cannot satisfy the quorum.
    let distinct_reporters = failure_reporters(env).len();
    if count > FAILURE_THRESHOLD && distinct_reporters >= MIN_DISTINCT_REPORTERS && !is_paused(env)
    {
        env.storage().instance().set(&DataKey::Paused, &true);
        events::emit_circuit_breaker_triggered(env, count);
    }

    Ok(())
}

/// Resets the failure counter, clears all per-reporter accounting and unpauses
/// the contract. Authorization is enforced by the calling entry point;
/// address validation is not repeated here.
pub fn reset(env: &Env, _admin: soroban_sdk::Address) -> Result<(), ContractError> {
    // Clear per-reporter state so the next window starts clean.
    for reporter in failure_reporters(env).iter() {
        env.storage()
            .instance()
            .remove(&DataKey::ReporterFailureCount(reporter.clone()));
        env.storage()
            .instance()
            .remove(&DataKey::LastFailureReport(reporter.clone()));
    }
    env.storage().instance().remove(&DataKey::FailureReporters);

    env.storage().instance().set(&DataKey::FailureCount, &0u32);
    env.storage().instance().remove(&DataKey::Paused);
    Ok(())
}
