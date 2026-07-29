//! # Vero Core Contracts
//!
//! Core contracts for the Vero protocol, providing reputation-weighted voting,
//! task registration and verification, token locking/unlocking for guardians,
//! reward stream management, and multi-sig contract upgrades.

#![no_std]
#![warn(missing_docs)]

mod circuit_breaker;
mod contracts;

/// Pure consensus logic.
pub mod consensus;

mod drips;

/// Contract event emitters.
pub mod events;

mod gas;
mod guardian;
mod migrate;
mod reentrancy;
mod reputation;
mod storage;
mod task;
mod timelock;
mod types;
mod utils;
mod validation;
mod vault;

pub use contracts::proxy_entry::{VeroContract, VeroContractClient};
pub use contracts::rbac::{grant_role_internal, has_role, require_role, revoke_role_internal};
pub use drips::{get_reward_stream, start_drips_stream};
pub use guardian::{add_guardian, get_all_guardians, is_guardian};
pub use task::{get_task, register_tasks};
pub use types::{
    BatchCall, ContractError, DataKey, GuardianEntry, Operation, RewardStream, Role, Snapshot,
    SnapshotMeta, Task,
};

/// Default weight threshold: a task requires at least 300 cumulative
/// reputation weight to be resolved. This can be overridden by the
/// admin via `set_weight_threshold`.
pub const DEFAULT_WEIGHT_THRESHOLD: u64 = 300;

/// Type alias for the main `VeroContract` implementation.
pub type VeroCore = VeroContract;
