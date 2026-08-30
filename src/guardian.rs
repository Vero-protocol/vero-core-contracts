#![allow(missing_docs)]

use soroban_sdk::{Address, Env, Vec};

use crate::types::{ContractError, DataKey};

const LEDGER_TTL: u32 = 100_000;

/// Adds a new guardian to the contract.
///
/// Address validation (`admin`, `guardian`) is performed by the calling
/// entrypoint; this helper must not repeat it.
pub fn add_guardian(env: &Env, _admin: Address, guardian: Address) -> Result<(), ContractError> {
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

    // Maintain a dense slot index alongside `AllGuardians` so paginated
    // reads (`get_guardians_page`) can fetch a bounded page of slots instead
    // of the whole guardian set. See `remove_guardian` for the matching
    // swap-remove compaction.
    let slot: u32 = env
        .storage()
        .instance()
        .get(&DataKey::GuardianIndexCount)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKey::GuardianIndexAt(slot), &guardian);
    env.storage()
        .instance()
        .set(&DataKey::GuardianIndexOf(guardian.clone()), &slot);
    env.storage()
        .instance()
        .set(&DataKey::GuardianIndexCount, &(slot + 1));

    env.storage().instance().set(&key, &true);
    env.storage().instance().extend_ttl(LEDGER_TTL, LEDGER_TTL);
    Ok(())
}

/// Internal helper that deregisters a guardian from all membership structures.
///
/// This removes the guardian from:
/// - The Guardian flag (DataKey::Guardian)
/// - The AllGuardians set
/// - The dense slot index (GuardianIndexAt/GuardianIndexOf/GuardianIndexCount)
///
/// Caller is responsible for any validation (e.g., NotGuardian check) and
/// peripheral cleanup (e.g., token refunds, timelock clearing).
pub(crate) fn deregister_guardian(env: &Env, guardian: Address) -> Result<(), ContractError> {
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

    // Swap-remove `guardian` from the dense slot index: move the last slot's
    // occupant into the freed slot, then shrink the count by one. Keeps
    // `get_guardians_page` bounded-cost without needing to shift every
    // subsequent slot on removal.
    if let Some(slot) = env
        .storage()
        .instance()
        .get::<_, u32>(&DataKey::GuardianIndexOf(guardian.clone()))
    {
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::GuardianIndexCount)
            .unwrap_or(0);
        if count > 0 {
            let last_slot = count - 1;
            if slot != last_slot {
                // SAFETY: `add_guardian` writes `GuardianIndexAt(slot)` for
                // every slot in `0..count`, and `remove_guardian` keeps the
                // index dense via swap-remove, so when `count > 0` the slot
                // `last_slot = count - 1` is always populated. Proven-safe
                // invariant.
                let last_addr: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::GuardianIndexAt(last_slot))
                    .unwrap();
                env.storage()
                    .instance()
                    .set(&DataKey::GuardianIndexAt(slot), &last_addr);
                env.storage()
                    .instance()
                    .set(&DataKey::GuardianIndexOf(last_addr), &slot);
            }
            env.storage()
                .instance()
                .remove(&DataKey::GuardianIndexAt(last_slot));
            env.storage()
                .instance()
                .remove(&DataKey::GuardianIndexOf(guardian.clone()));
            env.storage()
                .instance()
                .set(&DataKey::GuardianIndexCount, &last_slot);
        }
    }

    Ok(())
}

/// Removes an existing guardian from the contract.
///
/// Address validation (`admin`, `guardian`) is performed by the calling
/// entrypoint; this helper must not repeat it.
pub fn remove_guardian(env: &Env, _admin: Address, guardian: Address) -> Result<(), ContractError> {
    deregister_guardian(env, guardian)
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
