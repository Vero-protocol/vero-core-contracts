use soroban_sdk::Env;

use crate::types::{ContractError, DataKey};

/// Returns `Err(ContractPaused)` if the contract is currently paused.
///
/// Called at the top of every state-changing entry point so that a paused
/// contract immediately rejects all mutations without reaching business logic.
pub fn require_not_paused(env: &Env) -> Result<(), ContractError> {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}
