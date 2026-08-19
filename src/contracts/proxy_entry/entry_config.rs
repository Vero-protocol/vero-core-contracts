#![allow(missing_docs)]

//! Fee, treasury, vault and voting-threshold configuration entrypoints.
//!
//! Part of the `VeroContract` entrypoint surface; see
//! [`crate::contracts::proxy_entry`] for the overall layout.

use super::{VeroContract, VeroContractClient};
use crate::types::{ContractError, DataKey};
use crate::validation::validate_external_address as validate_address;
use crate::DEFAULT_WEIGHT_THRESHOLD;
use crate::{circuit_breaker, events};
use soroban_sdk::{contractimpl, panic_with_error, Address, Env};

#[contractimpl]
impl VeroContract {
    pub fn set_weight_threshold(
        env: Env,
        admin: Address,
        threshold: u64,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::ConfigManager)?;
        env.storage()
            .instance()
            .set(&DataKey::WeightThreshold, &threshold);
        events::emit_threshold_set(&env, &admin, threshold);
        Ok(())
    }

    pub fn get_weight_threshold(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::WeightThreshold)
            .unwrap_or(DEFAULT_WEIGHT_THRESHOLD)
    }

    pub fn set_vault_address(env: Env, admin: Address, vault: Address) {
        if validate_address(&env, &admin).is_err() {
            panic_with_error!(env, ContractError::InvalidAddress);
        }
        if validate_address(&env, &vault).is_err() {
            panic_with_error!(env, ContractError::InvalidAddress);
        }
        circuit_breaker::require_not_paused(&env).unwrap();
        // Use try-catch pattern via unwrap since this function has no Result return
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::ConfigManager)
            .unwrap();
        env.storage().instance().set(&DataKey::VaultAddress, &vault);
        events::emit_vault_set(&env, &admin, &vault);
    }

    pub fn set_fee_bps(env: Env, admin: Address, bps: u32) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::ConfigManager)?;
        if bps > 1000 {
            return Err(ContractError::InvalidConfig);
        }
        env.storage().instance().set(&DataKey::FeeBps, &bps);
        Ok(())
    }

    pub fn set_treasury_address(
        env: Env,
        admin: Address,
        treasury: Address,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &treasury)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::ConfigManager)?;
        env.storage()
            .instance()
            .set(&DataKey::TreasuryAddress, &treasury);
        Ok(())
    }
}
