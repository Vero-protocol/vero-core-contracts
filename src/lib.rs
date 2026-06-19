#![no_std]

mod contracts;

mod circuit_breaker;
#[cfg(any(feature = "verification", test))]
pub mod consensus;
mod drips;
pub mod events;
mod gas;
mod guardian;
mod reentrancy;
mod reputation;
mod storage;
mod task;
mod timelock;
mod types;
mod validation;
mod vault;

/// Primary Soroban contract type and generated client for Vero core calls.
pub use contracts::proxy_entry::{VeroContract, VeroContractClient};
/// Reward stream helpers used by integrations that read or start drips payouts.
pub use drips::{get_reward_stream, start_drips_stream};
/// Guardian registry helpers for adding, removing, and checking guardians.
pub use guardian::{add_guardian, is_guardian, remove_guardian};
/// Task helpers for registering and reading task state.
pub use task::{get_task, register_tasks};
/// Public contract data types shared with generated clients and tests.
pub use types::{BatchCall, ContractError, Operation};

/// Default accumulated guardian weight required before a task resolves.
pub const DEFAULT_WEIGHT_THRESHOLD: u64 = 300;

/// Backwards-compatible alias for the main Vero contract type.
pub type VeroCore = VeroContract;
