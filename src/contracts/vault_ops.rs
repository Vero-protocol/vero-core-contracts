//! Token/fee vault operations: lock, unlock, resign, and emergency recovery.
//!
//! This module owns every function that moves tokens between a guardian's
//! wallet and the contract escrow, applies the protocol fee, and manages the
//! withdrawal timelock.  The vote-processing and snapshot/pagination concerns
//! live in [`super::voting`] and [`super::snapshot`] respectively.

use crate::types::{ContractError, DataKey};
use crate::{circuit_breaker, events, guardian, timelock};
use soroban_sdk::{Address, Env};

pub(crate) fn lock_tokens(env: &Env, guardian: Address, amount: i128) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    let token: Address = env
        .storage()
        .instance()
        .get(&DataKey::TokenAddress)
        .ok_or(ContractError::NotInitialized)?;

    let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
    let mut net_amount = amount;

    if fee_bps > 0 {
        let fee_amount = (amount * (fee_bps as i128)) / 10000;
        if fee_amount > 0 {
            if let Some(treasury) = env
                .storage()
                .instance()
                .get::<_, Address>(&DataKey::TreasuryAddress)
            {
                let token_client = soroban_sdk::token::Client::new(env, &token);
                token_client.transfer(&guardian, &treasury, &fee_amount);
                net_amount = amount - fee_amount;
            }
        }
    }

    let token_client = soroban_sdk::token::Client::new(env, &token);
    token_client.transfer(&guardian, &env.current_contract_address(), &net_amount);
    let key = DataKey::LockedBalance(guardian.clone());
    let prev: i128 = env.storage().instance().get(&key).unwrap_or(0);
    env.storage().instance().set(&key, &(prev + net_amount));
    events::emit_tokens_locked(env, &guardian, amount);
    Ok(())
}

pub(crate) fn request_unlock(env: &Env, guardian: Address) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    timelock::initiate_withdrawal(env, guardian.clone());
    events::emit_timelock_started(env, &guardian);
    Ok(())
}

pub(crate) fn unlock_tokens(env: &Env, guardian: Address) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    if guardian::is_guardian(env, &guardian) {
        return Err(ContractError::StillGuardian);
    }

    // Check if timelock has expired
    timelock::check_timelock_expired(env, &guardian)?;

    let key = DataKey::LockedBalance(guardian.clone());
    let amount: i128 = env.storage().instance().get(&key).unwrap_or(0);
    if amount > 0 {
        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenAddress)
            .ok_or(ContractError::NotInitialized)?;
        let token_client = soroban_sdk::token::Client::new(env, &token);

        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
        let mut net_amount = amount;

        if fee_bps > 0 {
            let fee_amount = (amount * (fee_bps as i128)) / 10000;
            if fee_amount > 0 {
                if let Some(treasury) = env
                    .storage()
                    .instance()
                    .get::<_, Address>(&DataKey::TreasuryAddress)
                {
                    token_client.transfer(&env.current_contract_address(), &treasury, &fee_amount);
                    net_amount = amount - fee_amount;
                }
            }
        }

        token_client.transfer(&env.current_contract_address(), &guardian, &net_amount);
        env.storage().instance().set(&key, &0i128);
    }

    // Clear the timelock after successful withdrawal
    timelock::clear_timelock(env, &guardian);
    events::emit_tokens_unlocked(env, &guardian, amount);
    Ok(())
}

pub(crate) fn emergency_recover(
    env: &Env,
    admin: Address,
    recipient: Address,
    amount: i128,
) -> Result<(), ContractError> {
    crate::validation::validate_external_address(env, &recipient)?;
    crate::validation::validate_token_amount(amount)?;

    let token: Address = env
        .storage()
        .instance()
        .get(&DataKey::TokenAddress)
        .ok_or(ContractError::NotInitialized)?;
    let contract_address = env.current_contract_address();
    let token_client = soroban_sdk::token::Client::new(env, &token);
    token_client.transfer(&contract_address, &recipient, &amount);
    events::emit_emergency_recovery(env, &admin, &recipient, amount);
    Ok(())
}

pub(crate) fn resign_guardian(env: &Env, guardian: Address) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env)?;
    guardian.require_auth();
    if !guardian::is_guardian(env, &guardian) {
        return Err(ContractError::NotGuardian);
    }

    // Check if timelock has expired
    timelock::check_timelock_expired(env, &guardian)?;

    let g_key = DataKey::Guardian(guardian.clone());
    env.storage().instance().remove(&g_key);
    let key = DataKey::LockedBalance(guardian.clone());
    let amount: i128 = env.storage().instance().get(&key).unwrap_or(0);
    if amount > 0 {
        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenAddress)
            .ok_or(ContractError::NotInitialized)?;
        let token_client = soroban_sdk::token::Client::new(env, &token);

        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
        let mut net_amount = amount;

        if fee_bps > 0 {
            let fee_amount = (amount * (fee_bps as i128)) / 10000;
            if fee_amount > 0 {
                if let Some(treasury) = env
                    .storage()
                    .instance()
                    .get::<_, Address>(&DataKey::TreasuryAddress)
                {
                    token_client.transfer(&env.current_contract_address(), &treasury, &fee_amount);
                    net_amount = amount - fee_amount;
                }
            }
        }

        token_client.transfer(&env.current_contract_address(), &guardian, &net_amount);
        env.storage().instance().set(&key, &0i128);
    }

    // Clear the timelock after successful resignation
    timelock::clear_timelock(env, &guardian);
    events::emit_guardian_resigned(env, &guardian);
    Ok(())
}
