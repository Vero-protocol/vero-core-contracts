use soroban_sdk::{Address, Env};

use crate::limits::{
    MAX_LOCK_THRESHOLD, MAX_REPUTATION_SCORE, MAX_TASK_ID, MAX_TOKEN_AMOUNT, MAX_WEIGHT_THRESHOLD,
};
use crate::types::ContractError;

pub fn validate_external_address(env: &Env, address: &Address) -> Result<(), ContractError> {
    crate::utils::address::validate_address(env, address)?;
    if address == &env.current_contract_address() {
        return Err(ContractError::InvalidAddress);
    }
    Ok(())
}

pub fn validate_distinct_addresses(left: &Address, right: &Address) -> Result<(), ContractError> {
    if left == right {
        return Err(ContractError::InvalidAddress);
    }
    Ok(())
}

pub fn validate_admin_address(env: &Env, admin: &Address) -> Result<(), ContractError> {
    validate_external_address(env, admin)
}

pub fn validate_reward_stream_config(
    env: &Env,
    drips_address: &Address,
    contributor: &Address,
    task_id: u64,
) -> Result<(), ContractError> {
    validate_external_address(env, drips_address)?;
    validate_external_address(env, contributor)?;
    validate_distinct_addresses(drips_address, contributor)?;
    validate_task_id(task_id)
}

/// Validates that a given `task_id` falls within the allowed range (`1..=MAX_TASK_ID`).
///
/// Note: `task_id == 0` is intentionally rejected because `0` serves as a reserved
/// sentinel value across the protocol to indicate an uninitialized, unset, or
/// invalid task identifier.
pub fn validate_task_id(task_id: u64) -> Result<(), ContractError> {
    if task_id == 0 || task_id > MAX_TASK_ID {
        return Err(ContractError::InvalidConfig);
    }
    Ok(())
}

#[allow(dead_code)]
pub fn validate_token_amount(amount: i128) -> Result<(), ContractError> {
    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }
    if amount > MAX_TOKEN_AMOUNT {
        return Err(ContractError::InvalidRange);
    }
    Ok(())
}

#[allow(dead_code)]
pub fn validate_lock_threshold(lock_threshold: i128) -> Result<(), ContractError> {
    if lock_threshold <= 0 {
        return Err(ContractError::InvalidAmount);
    }
    if lock_threshold > MAX_LOCK_THRESHOLD {
        return Err(ContractError::InvalidRange);
    }
    Ok(())
}

pub fn validate_reputation_score(score: u64) -> Result<(), ContractError> {
    if score == 0 {
        return Err(ContractError::InvalidAmount);
    }
    if score > MAX_REPUTATION_SCORE {
        return Err(ContractError::InvalidRange);
    }
    Ok(())
}

#[allow(dead_code)]
pub fn validate_weight_threshold(threshold: u64) -> Result<(), ContractError> {
    if threshold == 0 {
        return Err(ContractError::InvalidAmount);
    }
    if threshold > MAX_WEIGHT_THRESHOLD {
        return Err(ContractError::InvalidRange);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_task_id_zero_rejected() {
        assert_eq!(validate_task_id(0), Err(ContractError::InvalidConfig));
    }

    #[test]
    fn test_validate_task_id_valid() {
        assert_eq!(validate_task_id(1), Ok(()));
        assert_eq!(validate_task_id(MAX_TASK_ID), Ok(()));
    }

    #[test]
    fn test_validate_task_id_exceeds_max_rejected() {
        assert_eq!(
            validate_task_id(MAX_TASK_ID + 1),
            Err(ContractError::InvalidConfig)
        );
    }
}
