#![allow(missing_docs)]

//! Reward-stream (drips) entrypoints.
//!
//! Part of the `VeroContract` entrypoint surface; see
//! [`crate::contracts::proxy_entry`] for the overall layout.

use super::{VeroContract, VeroContractClient};
use crate::types::{ContractError, RewardStream};
use crate::validation::validate_external_address as validate_address;
use crate::{circuit_breaker, drips, events};
use soroban_sdk::{contractimpl, Address, Env};

#[contractimpl]
impl VeroContract {
    pub fn start_reward_stream(
        env: Env,
        admin: Address,
        drips_address: Address,
        contributor: Address,
        task_id: u64,
    ) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        validate_address(&env, &drips_address)?;
        validate_address(&env, &contributor)?;
        circuit_breaker::require_not_paused(&env)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::TreasuryManager)?;

        let result = drips::start_drips_stream(&env, drips_address, contributor.clone(), task_id);

        match &result {
            Ok(()) => events::emit_reward_stream_started(&env, task_id, &contributor),
            Err(_) => events::emit_reward_stream_failed(&env, task_id, &contributor),
        }

        result
    }

    pub fn get_reward_stream(env: Env, task_id: u64) -> Option<RewardStream> {
        drips::get_reward_stream(&env, task_id)
    }
}
