#![allow(missing_docs)]

//! Contract upgrade entrypoints — both the immediate admin upgrade and the
//! multi-sig upgrade workflow.
//!
//! All entrypoints delegate to [`crate::contracts::upgrade`]; see there for
//! full documentation. Part of the `VeroContract` entrypoint surface; see
//! [`crate::contracts::proxy_entry`] for the overall layout.

use super::{VeroContract, VeroContractClient};
use crate::types::ContractError;
use soroban_sdk::{contractimpl, Address, BytesN, Env, Vec};

#[contractimpl]
impl VeroContract {
    /// Immediately replace the contract's WASM code. Callable only by the
    /// contract admin.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn upgrade_contract(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        crate::contracts::upgrade::upgrade_contract(env, admin, new_wasm_hash)
    }

    /// Configure the list of authorized upgrade signers and the required quorum.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn set_upgrade_signers(
        env: Env,
        admin: Address,
        signers: Vec<Address>,
        threshold: u32,
    ) -> Result<(), ContractError> {
        crate::contracts::upgrade::set_upgrade_signers(env, admin, signers, threshold)
    }

    /// Returns the currently configured list of authorized upgrade signers.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn get_upgrade_signers(env: Env) -> Vec<Address> {
        crate::contracts::upgrade::get_upgrade_signers(env)
    }

    /// Returns the minimum number of upgrade approvals required (quorum).
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn get_upgrade_threshold(env: Env) -> u32 {
        crate::contracts::upgrade::get_upgrade_threshold(env)
    }

    /// Propose a new upgrade WASM hash as an upgrade signer.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn propose_upgrade(
        env: Env,
        signer: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        crate::contracts::upgrade::propose_upgrade(env, signer, new_wasm_hash)
    }

    /// Approve a pending upgrade as an authorized signer.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn approve_upgrade(env: Env, signer: Address) -> Result<(), ContractError> {
        crate::contracts::upgrade::approve_upgrade(env, signer)
    }

    /// Execute the pending upgrade once the approval quorum is met.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn execute_upgrade(env: Env) -> Result<(), ContractError> {
        crate::contracts::upgrade::execute_upgrade(env)
    }

    /// Cancel a pending upgrade. Only the contract admin may call this.
    ///
    /// Delegates to [`crate::contracts::upgrade`].
    pub fn cancel_upgrade(env: Env, admin: Address) -> Result<(), ContractError> {
        crate::contracts::upgrade::cancel_upgrade(env, admin)
    }
}
