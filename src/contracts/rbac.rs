use crate::events;
use crate::types::{ContractError, DataKey, Role};
use soroban_sdk::{Address, Env};

/// Check whether an address holds a specific role.
///
/// Looks up `DataKey::RoleAssignment(address, role)` in instance storage.
/// Returns `true` if the role assignment exists and is set to `true`, `false` otherwise.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `address` - Target address being queried for role possession.
/// * `role` - The specific [`Role`] variant to check.
pub fn has_role(env: &Env, address: &Address, role: Role) -> bool {
    let key = DataKey::RoleAssignment(address.clone(), role);
    env.storage().instance().get(&key).unwrap_or(false)
}

/// Require that the caller holds a specific role, or revert with Unauthorized.
/// This is the "modifier" equivalent used at the start of privileged functions.
///
/// Calls `caller.require_auth()` to verify transaction signature/authorization,
/// then verifies role assignment via [`has_role`].
///
/// # Errors
/// * [`ContractError::NotAuthorized`] - Returned if caller lacks the requested role.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `caller` - Caller address executing the privileged operation.
/// * `role` - The required [`Role`].
pub fn require_role(env: &Env, caller: &Address, role: Role) -> Result<(), ContractError> {
    caller.require_auth();
    if !has_role(env, caller, role) {
        return Err(ContractError::NotAuthorized);
    }
    Ok(())
}

/// Count how many addresses currently hold the specified role.
/// Used for admin lockout prevention.
///
/// Note: Soroban storage does not support reverse lookups, so this function
/// checks a known set of addresses that could potentially hold roles.
/// For the Admin role, we maintain a small set of known role holders.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `role` - The [`Role`] whose total active assignees should be counted.
pub fn count_role_holders(env: &Env, role: Role) -> u32 {
    // For now, we scan all guardians + the stored admin address as potential role holders.
    // This is acceptable because:
    // 1. Admin role should have very few holders (typically 1-5)
    // 2. This function is only called during revoke_role, not on hot paths

    let mut count = 0u32;

    // Check the original admin address
    if let Some(admin) = env.storage().instance().get::<_, Address>(&DataKey::Admin) {
        if has_role(env, &admin, role) {
            count += 1;
        }
    }

    // Check all guardians (they could have been granted roles)
    let all_guardians: soroban_sdk::Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::AllGuardians)
        .unwrap_or(soroban_sdk::Vec::new(env));

    for guardian in all_guardians.iter() {
        if has_role(env, &guardian, role) {
            count += 1;
        }
    }

    // For a production system with many role holders, consider maintaining
    // a separate AllRoleHolders(Role) index updated on grant/revoke.

    count
}

/// Grant a role to a target address. Only callable by Admin role holders.
///
/// Sets `DataKey::RoleAssignment(target, role)` to `true` in instance storage
/// and emits a `RoleGranted` event.
///
/// # Errors
/// * [`ContractError::NotAuthorized`] - Returned if caller is not an Admin or fails auth.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `caller` - Admin address granting the role.
/// * `target` - Target address receiving the role.
/// * `role` - The [`Role`] to assign.
pub fn grant_role_internal(
    env: &Env,
    caller: &Address,
    target: &Address,
    role: Role,
) -> Result<(), ContractError> {
    require_role(env, caller, Role::Admin)?;

    let key = DataKey::RoleAssignment(target.clone(), role);
    env.storage().instance().set(&key, &true);

    events::emit_role_granted(env, caller, target, role as u8);

    Ok(())
}

/// Revoke a role from a target address. Only callable by Admin role holders.
/// Prevents removal of the last Admin role holder to avoid lockout.
///
/// Removes `DataKey::RoleAssignment(target, role)` from instance storage
/// and emits a `RoleRevoked` event.
///
/// # Errors
/// * [`ContractError::NotAuthorized`] - Returned if caller is not an Admin or fails auth.
/// * [`ContractError::LastAdminRemovalBlocked`] - Returned if attempting to revoke the last remaining Admin.
///
/// # Arguments
/// * `env` - Reference to the Soroban environment.
/// * `caller` - Admin address executing the revocation.
/// * `target` - Target address having the role removed.
/// * `role` - The [`Role`] to revoke.
pub fn revoke_role_internal(
    env: &Env,
    caller: &Address,
    target: &Address,
    role: Role,
) -> Result<(), ContractError> {
    require_role(env, caller, Role::Admin)?;

    // Admin lockout prevention: if revoking Admin role, ensure at least one remains
    if role == Role::Admin {
        let admin_count = count_role_holders(env, Role::Admin);
        if admin_count <= 1 {
            return Err(ContractError::LastAdminRemovalBlocked);
        }
    }

    let key = DataKey::RoleAssignment(target.clone(), role);
    env.storage().instance().remove(&key);

    events::emit_role_revoked(env, caller, target, role as u8);

    Ok(())
}
