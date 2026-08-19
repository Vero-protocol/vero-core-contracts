#![allow(missing_docs)]

//! Role-based access-control entrypoints.
//!
//! Part of the `VeroContract` entrypoint surface; see
//! [`crate::contracts::proxy_entry`] for the overall layout.

use super::{VeroContract, VeroContractClient};
use crate::types::ContractError;
use crate::validation::validate_external_address as validate_address;
use soroban_sdk::{contractimpl, Address, Env};

#[contractimpl]
impl VeroContract {
    /// Grant a role to a target address. Only callable by Admin role holders.
    ///
    /// # Errors
    /// * `NotAuthorized` — Caller does not hold the Admin role.
    pub fn grant_role(
        env: Env,
        caller: Address,
        target: Address,
        role: crate::types::Role,
    ) -> Result<(), ContractError> {
        validate_address(&env, &caller)?;
        validate_address(&env, &target)?;
        crate::contracts::rbac::grant_role_internal(&env, &caller, &target, role)
    }

    /// Revoke a role from a target address. Only callable by Admin role holders.
    ///
    /// # Errors
    /// * `NotAuthorized` — Caller does not hold the Admin role.
    /// * `LastAdminRemovalBlocked` — Cannot revoke the last remaining Admin role.
    pub fn revoke_role(
        env: Env,
        caller: Address,
        target: Address,
        role: crate::types::Role,
    ) -> Result<(), ContractError> {
        validate_address(&env, &caller)?;
        validate_address(&env, &target)?;
        crate::contracts::rbac::revoke_role_internal(&env, &caller, &target, role)
    }

    /// Check whether an address holds a specific role.
    pub fn has_role(env: Env, address: Address, role: crate::types::Role) -> bool {
        crate::contracts::rbac::has_role(&env, &address, role)
    }
}
