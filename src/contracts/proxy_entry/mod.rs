#![allow(missing_docs)]

//! Contract entrypoints for the Vero Core contract.
//!
//! `VeroContract`'s public entrypoints are organized into domain-grouped
//! `#[contractimpl] impl VeroContract` blocks, mirroring the domain split used
//! by the `tests/` directory and the `contracts::logic` / `contracts::rbac`
//! modules. Rust allows a single contract type to be implemented across
//! multiple `impl` blocks (and files), and Soroban's `#[contractimpl]` macro
//! supports the same. The domain blocks live in child modules so that the
//! macro-generated `VeroContractClient` impls (whose backing fields are
//! private to this module) remain in scope. See each domain module:
//!
//! * [`entry_circuit_breaker`] — pause & failure reporting
//! * [`entry_guardians`] — guardians & reputation
//! * [`entry_tokens`] — token locking & emergency recovery
//! * [`entry_config`] — fee / treasury / threshold config
//! * [`entry_tasks`] — task registration & voting
//! * [`entry_rewards`] — reward (drips) streams
//! * [`entry_upgrades`] — immediate & multi-sig upgrades
//! * [`entry_snapshots`] — snapshots & pagination
//! * [`entry_rbac`] — role-based access control
//!
//! This module retains the contract type definition plus the lifecycle,
//! migration and batch-dispatch entrypoints.

pub mod entry_circuit_breaker;
pub mod entry_config;
pub mod entry_guardians;
pub mod entry_rbac;
pub mod entry_rewards;
pub mod entry_snapshots;
pub mod entry_tasks;
pub mod entry_tokens;
pub mod entry_upgrades;

use crate::events;
use crate::types::{BatchCall, ContractError, DataKey};
use crate::validation::{validate_external_address as validate_address, validate_lock_threshold};
use soroban_sdk::{contract, contractimpl, Address, Env};

/// The main entrypoint for the Vero Core contract.
///
/// Implements all contract features including voting, task registration,
/// reputation management, token locking, and upgrades.
#[contract]
pub struct VeroContract;

// ─── Lifecycle & initialization ──────────────────────────────────────────

#[contractimpl]
impl VeroContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        lock_threshold: i128,
    ) -> Result<(), ContractError> {
        // Authenticate the account being installed as admin. Without this, an
        // observer can front-run initialization on a deployed-but-uninitialized
        // contract and install themselves as Admin.
        admin.require_auth();

        validate_address(&env, &admin)?;
        validate_address(&env, &token)?;
        validate_lock_threshold(lock_threshold)?;

        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            return Err(ContractError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TokenAddress, &token);
        env.storage()
            .instance()
            .set(&DataKey::LockThreshold, &lock_threshold);
        env.storage().instance().set(&DataKey::Paused, &false);

        // Grant Admin role to the deployer/initial admin
        let admin_role_key = DataKey::RoleAssignment(admin.clone(), crate::types::Role::Admin);
        env.storage().instance().set(&admin_role_key, &true);

        crate::migrate::set_version(&env, crate::migrate::CURRENT_VERSION);

        env.storage().instance().extend_ttl(100_000, 100_000);
        events::emit_contract_initialized(&env, &admin);
        Ok(())
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    pub fn get_estimated_cost(_env: Env, op: crate::types::Operation) -> u64 {
        crate::gas::get_estimated_cost(op)
    }

    /// Returns the currently recorded storage version.
    pub fn get_storage_version(env: Env) -> u32 {
        crate::migrate::get_version(&env)
    }

    /// Run the storage migration to bring the storage schema to the latest version.
    /// Only contract admin can trigger migration.
    pub fn migrate_storage(env: Env, admin: Address) -> Result<(), ContractError> {
        validate_address(&env, &admin)?;
        crate::contracts::rbac::require_role(&env, &admin, crate::types::Role::Admin)?;
        crate::migrate::migrate(&env)
    }
}

// ─── Batch dispatch ──────────────────────────────────────────────────────

#[contractimpl]
impl VeroContract {
    pub fn batch_execute(
        env: Env,
        calls: soroban_sdk::Vec<BatchCall>,
    ) -> Result<(), ContractError> {
        // Bound the batch by its total estimated instruction cost so a caller
        // can't submit more work than one transaction can execute. The
        // `BatchCall::operation()` -> `gas::get_estimated_cost()` mapping is
        // the single source of truth linking each batchable call to its cost,
        // so a new `BatchCall` variant can't be dispatched without a
        // registered gas estimate.
        let mut estimated_cost: u64 = 0;
        for call in calls.iter() {
            estimated_cost =
                estimated_cost.saturating_add(crate::gas::get_estimated_cost(call.operation()));
            if estimated_cost > crate::gas::MAX_BATCH_EXECUTE_COST {
                return Err(ContractError::BatchTooLarge);
            }
        }

        for call in calls.iter() {
            match call {
                BatchCall::RegisterTask(admin, task_id, min_votes_required) => {
                    Self::register_task(env.clone(), admin, task_id, min_votes_required)?
                }
                BatchCall::CancelTask(admin, task_id) => {
                    Self::cancel_task(env.clone(), admin, task_id)?
                }
                BatchCall::Vote(guardian, task_id) => Self::vote(env.clone(), guardian, task_id)?,
                BatchCall::AddGuardian(admin, guardian) => {
                    Self::add_guardian(env.clone(), admin, guardian)?
                }
                BatchCall::RemoveGuardian(admin, guardian) => {
                    Self::remove_guardian(env.clone(), admin, guardian)?
                }
                BatchCall::SetReputation(admin, guardian, score) => {
                    Self::set_reputation(env.clone(), admin, guardian, score)?
                }
                BatchCall::LockTokens(guardian, amount) => {
                    Self::lock_tokens(env.clone(), guardian, amount)?
                }
                BatchCall::RequestUnlock(guardian) => Self::request_unlock(env.clone(), guardian)?,
                BatchCall::UnlockTokens(guardian) => Self::unlock_tokens(env.clone(), guardian)?,
                BatchCall::ResignGuardian(guardian) => {
                    Self::resign_guardian(env.clone(), guardian)?
                }
                BatchCall::SetWeightThreshold(admin, threshold) => {
                    Self::set_weight_threshold(env.clone(), admin, threshold)?
                }
                BatchCall::SetVaultAddress(admin, vault) => {
                    Self::set_vault_address(env.clone(), admin, vault)?
                }
                BatchCall::SetUpgradeSigners(admin, signers, threshold) => {
                    Self::set_upgrade_signers(env.clone(), admin, signers, threshold)?
                }
                BatchCall::ProposeUpgrade(signer, hash) => {
                    Self::propose_upgrade(env.clone(), signer, hash)?
                }
                BatchCall::ApproveUpgrade(signer) => Self::approve_upgrade(env.clone(), signer)?,
                BatchCall::ExecuteUpgrade => Self::execute_upgrade(env.clone())?,
                BatchCall::CancelUpgrade(admin) => Self::cancel_upgrade(env.clone(), admin)?,
                BatchCall::StartRewardStream(admin, drips, contributor, task_id) => {
                    Self::start_reward_stream(env.clone(), admin, drips, contributor, task_id)?
                }
                BatchCall::TogglePause(admin) => Self::toggle_pause(env.clone(), admin)?,
                BatchCall::Pause(admin) => Self::pause(env.clone(), admin)?,
                BatchCall::Unpause(admin) => Self::unpause(env.clone(), admin)?,
                BatchCall::RecordFailure => crate::circuit_breaker::record_failure_anonymous(&env)?,
                BatchCall::ResetCircuitBreaker(admin) => {
                    Self::reset_circuit_breaker(env.clone(), admin)?;
                }
                BatchCall::EmergencyRecover(admin, recipient, amount) => {
                    Self::emergency_recover(env.clone(), admin, recipient, amount)?
                }
                BatchCall::SetFeeBps(admin, bps) => Self::set_fee_bps(env.clone(), admin, bps)?,
                BatchCall::SetTreasuryAddress(admin, treasury) => {
                    Self::set_treasury_address(env.clone(), admin, treasury)?
                }
            }
        }
        Ok(())
    }
}
