use crate::types::{ContractError, DataKey};
use soroban_sdk::{Address, Env};

const WITHDRAWAL_DELAY_SECONDS: u64 = 86400; // 24 hours

pub fn initiate_withdrawal(env: &Env, guardian: Address) {
    let key = DataKey::WithdrawalTimelock(guardian);
    let current_time = env.ledger().timestamp();
    env.storage().instance().set(&key, &current_time);
}

pub fn check_timelock_expired(env: &Env, guardian: &Address) -> Result<(), ContractError> {
    let key = DataKey::WithdrawalTimelock(guardian.clone());
    match env.storage().instance().get::<_, u64>(&key) {
        Some(initiation_time) => {
            let current_time = env.ledger().timestamp();
            if current_time < initiation_time + WITHDRAWAL_DELAY_SECONDS {
                return Err(ContractError::WithdrawalTimelockActive);
            }
            Ok(())
        }
        None => Err(ContractError::WithdrawalTimelockActive), // No timelock set — must call request_unlock first
    }
}

pub fn clear_timelock(env: &Env, guardian: &Address) {
    let key = DataKey::WithdrawalTimelock(guardian.clone());
    env.storage().instance().remove(&key);
}
