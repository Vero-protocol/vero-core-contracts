#![allow(missing_docs)]

//! Circuit-breaker control entrypoints: pause/unpause and failure reporting.
//!
//! Part of the `VeroContract` entrypoint surface; see
//! [`crate::contracts::proxy_entry`] for the overall layout.

use super::{VeroContract, VeroContractClient};
use crate::types::{ContractError, DataKey};
use crate::validation::validate_external_address as validate_address;
use crate::{circuit_breaker, events};
use soroban_sdk::{contractimpl, Address, Env, Vec};

#[contractimpl]
impl VeroContract {
    pub fn toggle_pause(env: Env, admin: Address) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::EmergencyManager)?;
        let current = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        let new_paused = !current;
        env.storage().instance().set(&DataKey::Paused, &new_paused);
        events::emit_pause_toggled(&env, new_paused);
        Ok(())
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::EmergencyManager)?;
        env.storage().instance().set(&DataKey::Paused, &true);
        events::emit_pause_toggled(&env, true);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::EmergencyManager)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        events::emit_pause_toggled(&env, false);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Report an observed failure to the circuit breaker.
    ///
    /// Reporting stays open to any observer, but every report is now
    /// **authenticated, rate-limited and quota-capped per address**, and the
    /// breaker only auto-pauses once several *independent* reporters agree.
    /// This preserves the "any observer can report" design goal while making it
    /// impossible for a single address to unilaterally pause the contract.
    ///
    /// See [`crate::circuit_breaker`] for the full trust-model decision record.
    ///
    /// # Errors
    /// * `InvalidAddress` — reporter is the zero address or the contract itself.
    /// * `UnauthorizedReporter` — trusted-reporters-only mode is enabled and the
    ///   caller is not a guardian / EmergencyManager / Admin.
    /// * `ReportRateLimited` — the caller reported within the cooldown window.
    /// * `ReporterQuotaExceeded` — the caller exhausted its per-window quota.
    pub fn record_failure(env: Env, reporter: Address) -> Result<(), ContractError> {
        validate_address(&env, &reporter)?;
        circuit_breaker::record_failure(&env, reporter)
    }

    /// Current cumulative failure count for the active breaker window.
    pub fn get_failure_count(env: Env) -> u32 {
        circuit_breaker::failure_count(&env)
    }

    /// Number of reports the given address contributed to the active window.
    pub fn get_reporter_failure_count(env: Env, reporter: Address) -> u32 {
        circuit_breaker::reporter_count(&env, &reporter)
    }

    /// Distinct addresses that have reported failures in the active window.
    pub fn get_failure_reporters(env: Env) -> Vec<Address> {
        circuit_breaker::failure_reporters(&env)
    }

    /// Whether failure reporting is currently restricted to trusted monitors.
    pub fn is_trusted_reporters_only(env: Env) -> bool {
        circuit_breaker::trusted_reporters_only(&env)
    }

    /// Restrict (or re-open) failure reporting to trusted monitors — registered
    /// guardians and `EmergencyManager` / `Admin` role holders.
    ///
    /// Intended as an escape hatch if a Sybil flood of reports is ever observed.
    ///
    /// # Errors
    /// * `NotAuthorized` — caller does not hold the `EmergencyManager` role.
    pub fn set_trusted_reporters_only(
        env: Env,
        admin: Address,
        enabled: bool,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::EmergencyManager)?;
        circuit_breaker::set_trusted_reporters_only(&env, enabled);
        events::emit_trusted_reporters_only_set(&env, &admin, enabled);
        Ok(())
    }

    pub fn reset_circuit_breaker(env: Env, admin: Address) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::EmergencyManager)?;
        circuit_breaker::reset(&env, admin.clone())?;
        events::emit_circuit_breaker_reset(&env, &admin);
        Ok(())
    }
}
