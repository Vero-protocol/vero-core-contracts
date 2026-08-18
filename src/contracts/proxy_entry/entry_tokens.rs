#![allow(missing_docs)]

//! Token locking, unlocking and emergency-recovery entrypoints.
//!
//! Part of the `VeroContract` entrypoint surface; see
//! [`crate::contracts::proxy_entry`] for the overall layout.

use super::{VeroContract, VeroContractClient};
use crate::contracts::logic;
use crate::types::{ContractError, DataKey};
use crate::validation::validate_external_address as validate_address;
use soroban_sdk::{contractimpl, Address, Env};

#[contractimpl]
impl VeroContract {
    pub fn lock_tokens(env: Env, guardian: Address, amount: i128) -> Result<(), ContractError> {
        validate_address(&env, &guardian)?;
        logic::lock_tokens(&env, guardian, amount)
    }

    pub fn request_unlock(env: Env, guardian: Address) -> Result<(), ContractError> {
        validate_address(&env, &guardian)?;
        logic::request_unlock(&env, guardian)
    }

    pub fn unlock_tokens(env: Env, guardian: Address) -> Result<(), ContractError> {
        validate_address(&env, &guardian)?;
        logic::unlock_tokens(&env, guardian)
    }

    /// Recovers tokens from the contract in emergency situations.
    ///
    /// Note: This function deliberately bypasses the circuit breaker pause gate
    /// (`require_not_paused`), as it serves as the recovery mechanism of last resort
    /// when normal contract operations are halted or paused. Requires the caller
    /// to hold the `EmergencyManager` role.
    pub fn emergency_recover(
        env: Env,
        admin: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &recipient)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::EmergencyManager)?;
        logic::emergency_recover(&env, admin, recipient, amount)
    }

    pub fn get_withdrawal_timelock(env: Env, guardian: Address) -> Option<u64> {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawalTimelock(guardian))
    }
}
