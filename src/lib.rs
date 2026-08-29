//! # Vero Core Contracts
//!
//! Core contracts for the Vero protocol, providing reputation-weighted voting,
//! task registration and verification, token locking/unlocking for guardians,
//! reward stream management, and multi-sig contract upgrades.

#![no_std]
#![warn(missing_docs)]

mod circuit_breaker;
// The Soroban contract surface (entrypoints, shared logic, RBAC, storage
// layout and upgrades) lives under `contracts/`; see `contracts/mod.rs` for
// the documented boundary. Domain primitives composed by that surface stay at
// the crate root.
mod contracts;

/// Pure consensus logic.
pub mod consensus;

mod drips;

/// Contract event emitters.
pub mod events;

/// Instruction-cost estimates for public entry points.
pub mod gas;
mod guardian;
/// Protocol-wide limit constants.
pub mod limits;
/// Storage migration and atomic pre-flight validation.
pub mod migrate;
mod reentrancy;
mod reputation;
mod storage;
mod task;
mod timelock;
mod types;
mod utils;
/// Parameter and address validation helpers.
pub mod validation;

pub use contracts::proxy_entry::{VeroContract, VeroContractClient};
pub use limits::MAX_WEIGHT_THRESHOLD;
pub use types::{
    BatchCall, ContractError, DataKey, GuardianEntry, Operation, RewardStream, Role, Snapshot,
    SnapshotMeta, Task,
};

pub use circuit_breaker::FAILURE_THRESHOLD;
pub use storage::ARCHIVE_AFTER_SECONDS;
pub use utils::address::ZERO_ADDRESS_STR;

/// Default weight threshold: a task requires at least 300 cumulative
/// reputation weight to be resolved. This can be overridden by a
/// `ConfigManager` via `set_weight_threshold` (bounded to `1..=MAX_WEIGHT_THRESHOLD`).
pub const DEFAULT_WEIGHT_THRESHOLD: u64 = 300;

/// Type alias for the main `VeroContract` implementation.
pub type VeroCore = VeroContract;
