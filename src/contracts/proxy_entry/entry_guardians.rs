#![allow(missing_docs)]

//! Guardian membership and reputation entrypoints.
//!
//! Part of the `VeroContract` entrypoint surface; see
//! [`crate::contracts::proxy_entry`] for the overall layout.

use super::{VeroContract, VeroContractClient};
use crate::contracts::logic;
use crate::types::ContractError;
use crate::validation::validate_external_address as validate_address;
use crate::{circuit_breaker, events, guardian, reputation};
use soroban_sdk::{contractimpl, Address, Env};

#[contractimpl]
impl VeroContract {
    pub fn add_guardian(env: Env, admin: Address, guardian: Address) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &guardian)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::GuardianManager)?;
        guardian::add_guardian(&env, admin.clone(), guardian.clone())?;
        events::emit_guardian_added(&env, &admin, &guardian);
        Ok(())
    }

    pub fn remove_guardian(
        env: Env,
        admin: Address,
        guardian: Address,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &guardian)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::GuardianManager)?;
        guardian::remove_guardian(&env, admin.clone(), guardian.clone())?;
        events::emit_guardian_removed(&env, &admin, &guardian);
        Ok(())
    }

    pub fn is_guardian(env: Env, guardian: Address) -> bool {
        guardian::is_guardian(&env, &guardian)
    }

    pub fn set_reputation(
        env: Env,
        admin: Address,
        guardian: Address,
        score: u64,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &guardian)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::GuardianManager)?;
        reputation::set_reputation(&env, admin.clone(), guardian.clone(), score)?;
        events::emit_reputation_set(&env, &admin, &guardian, score);
        Ok(())
    }

    pub fn get_reputation(env: Env, guardian: Address) -> Option<u64> {
        reputation::get_reputation(&env, &guardian)
    }

    pub fn resign_guardian(env: Env, guardian: Address) -> Result<(), ContractError> {
        validate_address(&env, &guardian)?;
        logic::resign_guardian(&env, guardian)
    }
}
