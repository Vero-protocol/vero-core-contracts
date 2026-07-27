#![allow(missing_docs)]

use soroban_sdk::{Address, Env, Vec};

use crate::types::{ContractError, DataKey};
use crate::validation;

const LEDGER_TTL: u32 = 100_000;

/// Adds a new guardian to the contract.
pub fn add_guardian(env: &Env, admin: Address, guardian: Address) -> Result<(), ContractError> {
    validation::validate_guardian_config(env, &admin, &guardian)?;

    let key = DataKey::Guardian(guardian.clone());
    if env.storage().instance().has(&key) {
        return Err(ContractError::DuplicateGuardian);
    }

    let mut all_guardians: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::AllGuardians)
        .unwrap_or(Vec::new(env));

    if all_guardians.contains(guardian.clone()) {
        return Err(ContractError::DuplicateGuardian);
    }

    all_guardians.push_back(guardian.clone());
    env.storage()
        .instance()
        .set(&DataKey::AllGuardians, &all_guardians);

    env.storage().instance().set(&key, &true);
    env.storage().instance().extend_ttl(LEDGER_TTL, LEDGER_TTL);
    Ok(())
}

/// Removes an existing guardian from the contract.
pub fn remove_guardian(env: &Env, admin: Address, guardian: Address) -> Result<(), ContractError> {
    validation::validate_admin_address(env, &admin)?;
    validation::validate_external_address(env, &guardian)?;

    let key = DataKey::Guardian(guardian.clone());
    if !env.storage().instance().has(&key) {
        return Err(ContractError::NotGuardian);
    }

    env.storage().instance().remove(&key);

    let all_guardians: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::AllGuardians)
        .unwrap_or(Vec::new(env));

    let mut updated = Vec::new(env);
    for g in all_guardians.iter() {
        if g != guardian {
            updated.push_back(g.clone());
        }
    }
    env.storage()
        .instance()
        .set(&DataKey::AllGuardians, &updated);

    Ok(())
}

/// Checks if a given address is a registered guardian.
pub fn is_guardian(env: &Env, guardian: &Address) -> bool {
    let key = DataKey::Guardian(guardian.clone());
    env.storage().instance().get(&key).unwrap_or(false)
}

/// Retrieves a list of all registered guardians.
pub fn get_all_guardians(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::AllGuardians)
        .unwrap_or(Vec::new(env))
}
