//! Contract upgrade management: immediate single-admin upgrades and the
//! multi-sig upgrade proposal/approval/execution flow.
//!
//! Extracted from `proxy_entry.rs` so the contract's upgrade path — the most
//! security-sensitive code in the contract — lives in one module and can be
//! reviewed in isolation. The public entry points in `proxy_entry.rs` delegate
//! to the functions here; the exported contract API is unchanged.

use crate::contracts::rbac::require_role;
use crate::contracts::validate_address;
use crate::events;
use crate::types::{ContractError, DataKey, Role};
use soroban_sdk::{panic_with_error, Address, BytesN, Env, Vec};

/// Returns `true` iff `addrs` is strictly ordered, i.e. every address is
/// strictly greater than its predecessor (with no duplicates).
///
/// Empty and single-element lists are trivially sorted.
fn is_strictly_sorted_addresses(addrs: &Vec<Address>) -> bool {
    if addrs.len() < 2 {
        return true;
    }

    let mut prev = addrs.get(0).unwrap();
    let mut i = 1;
    while i < addrs.len() {
        let current = addrs.get(i).unwrap();
        if prev >= current {
            return false;
        }
        prev = current;
        i += 1;
    }

    true
}

/// Immediately replace the contract's WASM code. Callable only by the
/// contract admin.
///
/// This is the single-admin upgrade path; the multi-sig flow below
/// (`set_upgrade_signers` / `propose_upgrade` / `approve_upgrade` /
/// `execute_upgrade`) offers the quorum-gated alternative.
pub fn upgrade_contract(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
    if validate_address(&env, &admin).is_err() {
        panic_with_error!(env, ContractError::InvalidAddress);
    }
    require_role(&env, &admin, Role::Admin).unwrap();
    env.deployer()
        .update_current_contract_wasm(new_wasm_hash.clone());
    events::emit_contract_upgraded(&env, &admin, &new_wasm_hash);
}

/// Configure the list of authorized upgrade signers and the required quorum.
///
/// Only the contract admin may call this function. It overwrites any previous
/// multi-sig configuration and clears any pending upgrade proposal.
///
/// # Arguments
/// * `signers`   — List of addresses authorized to propose/approve upgrades.
/// * `threshold` — Minimum number of approvals required to execute an upgrade.
///
/// # Errors
/// * `NotAuthorized` — Caller is not the contract admin.
/// * `InvalidUpgradeConfig` — Threshold is zero or exceeds the number of signers.
pub fn set_upgrade_signers(
    env: Env,
    admin: Address,
    signers: Vec<Address>,
    threshold: u32,
) -> Result<(), ContractError> {
    validate_address(&env, &admin)?;
    for signer in signers.iter() {
        validate_address(&env, &signer)?;
    }
    require_role(&env, &admin, Role::Admin)?;

    if threshold == 0 || threshold > signers.len() || !is_strictly_sorted_addresses(&signers) {
        return Err(ContractError::InvalidUpgradeConfig);
    }

    // Clear any pending upgrade when reconfiguring
    env.storage()
        .instance()
        .remove(&DataKey::PendingUpgradeWasm);
    env.storage()
        .instance()
        .remove(&DataKey::PendingUpgradeApprovals);

    env.storage()
        .instance()
        .set(&DataKey::UpgradeSigners, &signers);
    env.storage()
        .instance()
        .set(&DataKey::UpgradeThreshold, &threshold);

    events::emit_upgrade_signers_set(&env, signers.len(), threshold);
    Ok(())
}

/// Returns the currently configured list of authorized upgrade signers.
pub fn get_upgrade_signers(env: Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::UpgradeSigners)
        .unwrap_or(Vec::new(&env))
}

/// Returns the minimum number of upgrade approvals required (quorum).
pub fn get_upgrade_threshold(env: Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::UpgradeThreshold)
        .unwrap_or(0u32)
}

/// Propose a new upgrade WASM hash as an upgrade signer.
///
/// If no pending upgrade exists, creates one and records the caller's
/// approval. The caller is added to the approvals list.
///
/// If a pending upgrade exists with a **different** WASM hash, the call
/// reverts. If the hash matches, the caller is added to the approval list
/// (same effect as calling `approve_upgrade`).
///
/// # Errors
/// * `NotUpgradeSigner` — Caller is not in the authorized signers list.
/// * `NoPendingUpgrade` — (not applicable; propose creates one).
/// * `AlreadyApproved` — Caller has already approved.
pub fn propose_upgrade(
    env: Env,
    signer: Address,
    new_wasm_hash: BytesN<32>,
) -> Result<(), ContractError> {
    validate_address(&env, &signer)?;
    signer.require_auth();

    // Verify signer is authorized
    let signers: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::UpgradeSigners)
        .ok_or(ContractError::NotUpgradeSigner)?;
    if !signers.contains(signer.clone()) {
        return Err(ContractError::NotUpgradeSigner);
    }

    // Check if there's an existing pending upgrade
    if let Some(existing_hash) = env
        .storage()
        .instance()
        .get::<_, BytesN<32>>(&DataKey::PendingUpgradeWasm)
    {
        // If hashes differ, reject
        if existing_hash != new_wasm_hash {
            return Err(ContractError::InvalidUpgradeConfig);
        }
        // Hash matches — just add approval (same as approve_upgrade but without require_auth)
        let mut approvals: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgradeApprovals)
            .unwrap_or(Vec::new(&env));

        if approvals.contains(signer.clone()) {
            return Err(ContractError::AlreadyApproved);
        }
        if let Some(previous) = approvals.last() {
            if previous >= signer {
                return Err(ContractError::InvalidUpgradeConfig);
            }
        }

        approvals.push_back(signer.clone());
        env.storage()
            .instance()
            .set(&DataKey::PendingUpgradeApprovals, &approvals);

        let threshold: u32 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeThreshold)
            .unwrap_or(0u32);

        events::emit_upgrade_approved(&env, &signer, approvals.len(), threshold);
        return Ok(());
    }

    // No pending upgrade — create one
    env.storage()
        .instance()
        .set(&DataKey::PendingUpgradeWasm, &new_wasm_hash);

    // Record the first approval
    let mut approvals: Vec<Address> = Vec::new(&env);
    approvals.push_back(signer.clone());
    env.storage()
        .instance()
        .set(&DataKey::PendingUpgradeApprovals, &approvals);

    events::emit_upgrade_proposed(&env, &signer);

    let threshold: u32 = env
        .storage()
        .instance()
        .get(&DataKey::UpgradeThreshold)
        .unwrap_or(0u32);
    events::emit_upgrade_approved(&env, &signer, approvals.len(), threshold);

    Ok(())
}

/// Approve a pending upgrade as an authorized signer.
///
/// A pending upgrade must exist. If the caller has already approved,
/// the call reverts with `AlreadyApproved`.
///
/// # Errors
/// * `NotUpgradeSigner` — Caller is not in the authorized signers list.
/// * `NoPendingUpgrade` — No upgrade has been proposed.
/// * `AlreadyApproved` — Caller has already approved this proposal.
pub fn approve_upgrade(env: Env, signer: Address) -> Result<(), ContractError> {
    validate_address(&env, &signer)?;
    signer.require_auth();

    // Verify signer is authorized
    let signers: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::UpgradeSigners)
        .ok_or(ContractError::NotUpgradeSigner)?;
    if !signers.contains(signer.clone()) {
        return Err(ContractError::NotUpgradeSigner);
    }

    // Verify there is a pending upgrade
    if !env.storage().instance().has(&DataKey::PendingUpgradeWasm) {
        return Err(ContractError::NoPendingUpgrade);
    }

    // Verify caller hasn't already approved
    let mut approvals: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::PendingUpgradeApprovals)
        .unwrap_or(Vec::new(&env));

    if approvals.contains(signer.clone()) {
        return Err(ContractError::AlreadyApproved);
    }
    if let Some(previous) = approvals.last() {
        if previous >= signer {
            return Err(ContractError::InvalidUpgradeConfig);
        }
    }

    approvals.push_back(signer.clone());
    env.storage()
        .instance()
        .set(&DataKey::PendingUpgradeApprovals, &approvals);

    let threshold: u32 = env
        .storage()
        .instance()
        .get(&DataKey::UpgradeThreshold)
        .unwrap_or(0u32);
    events::emit_upgrade_approved(&env, &signer, approvals.len(), threshold);

    Ok(())
}

/// Execute the pending upgrade once the approval quorum is met.
///
/// # Errors
/// * `NoPendingUpgrade` — No upgrade has been proposed.
/// * `UpgradeThresholdNotMet` — Not enough approvals yet.
pub fn execute_upgrade(env: Env) -> Result<(), ContractError> {
    // Check pending proposal exists
    let wasm_hash: BytesN<32> = env
        .storage()
        .instance()
        .get(&DataKey::PendingUpgradeWasm)
        .ok_or(ContractError::NoPendingUpgrade)?;

    let approvals: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::PendingUpgradeApprovals)
        .ok_or(ContractError::NoPendingUpgrade)?;

    let threshold: u32 = env
        .storage()
        .instance()
        .get(&DataKey::UpgradeThreshold)
        .ok_or(ContractError::InvalidUpgradeConfig)?;

    if approvals.len() < threshold {
        return Err(ContractError::UpgradeThresholdNotMet);
    }

    // Clean up pending state BEFORE upgrade (after upgrade the contract
    // code is replaced and further cleanup may not run).
    env.storage()
        .instance()
        .remove(&DataKey::PendingUpgradeWasm);
    env.storage()
        .instance()
        .remove(&DataKey::PendingUpgradeApprovals);

    events::emit_upgrade_executed(&env);

    // Perform the actual WASM upgrade
    env.deployer().update_current_contract_wasm(wasm_hash);

    Ok(())
}

/// Cancel a pending upgrade. Only the contract admin may call this.
///
/// # Errors
/// * `NotAuthorized` — Caller is not the contract admin.
/// * `NoPendingUpgrade` — No upgrade has been proposed.
pub fn cancel_upgrade(env: Env, admin: Address) -> Result<(), ContractError> {
    validate_address(&env, &admin)?;
    require_role(&env, &admin, Role::Admin)?;

    if !env.storage().instance().has(&DataKey::PendingUpgradeWasm) {
        return Err(ContractError::NoPendingUpgrade);
    }

    env.storage()
        .instance()
        .remove(&DataKey::PendingUpgradeWasm);
    env.storage()
        .instance()
        .remove(&DataKey::PendingUpgradeApprovals);

    events::emit_upgrade_cancelled(&env);
    Ok(())
}
