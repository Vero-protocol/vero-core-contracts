use crate::guardian;
use crate::types::{ContractError, DataKey};
use crate::validation;
use soroban_sdk::{Address, Env};

/// Minimum reputation score required to cast a vote.
/// Scores below this threshold are considered "low reputation" and are rejected.
pub const MIN_REPUTATION_THRESHOLD: u64 = 100;

/// Sets the reputation score for a guardian. Only callable by admin.
///
/// Address validation (`admin`, `guardian`) and the GuardianManager role
/// check are performed by the calling entrypoint; this helper must not
/// repeat them.
///
/// # Arguments
/// * `env` - The contract environment.
/// * `admin` - The admin address (must pass `require_auth`).
/// * `guardian` - The guardian whose reputation is being set.
/// * `score` - The u64 reputation score to assign.
pub fn set_reputation(
    env: &Env,
    _admin: Address,
    guardian: Address,
    score: u64,
) -> Result<(), ContractError> {
    validation::validate_reputation_score(score)?;

    if !guardian::is_guardian(env, &guardian) {
        return Err(ContractError::NotGuardian);
    }

    let key = DataKey::Reputation(guardian);
    env.storage().instance().set(&key, &score);
    Ok(())
}

/// Retrieves the raw reputation score for a guardian, if one exists.
pub fn get_reputation(env: &Env, guardian: &Address) -> Option<u64> {
    let key = DataKey::Reputation(guardian.clone());
    env.storage().instance().get(&key)
}

/// Returns the reputation score for `guardian`, or an error if absent or below
/// [`MIN_REPUTATION_THRESHOLD`].
///
/// This is the gate called by `vote()` to enforce the high-reputation invariant.
pub fn get_rep(env: &Env, guardian: &Address) -> Result<u64, ContractError> {
    let score = get_reputation(env, guardian).ok_or(ContractError::NoReputationScore)?;
    if score < MIN_REPUTATION_THRESHOLD {
        return Err(ContractError::InsufficientReputation);
    }
    Ok(score)
}
