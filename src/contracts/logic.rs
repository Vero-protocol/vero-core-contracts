//! Public re-exports for the three domain-focused sub-modules.
//!
//! `logic.rs` used to be a single 678-line file mixing token/fee vault
//! operations, vote processing, and snapshot/pagination.  Each concern now
//! lives in its own file:
//!
//! | Sub-module          | Responsibility                                          |
//! |---------------------|---------------------------------------------------------|
//! | [`vault_ops`]       | `lock_tokens`, `unlock_tokens`, `resign_guardian`, …    |
//! | [`voting`]          | `process_vote`, `vote_inner`, `process_vote_batch`, …   |
//! | [`snapshot`]        | `get_snapshot`, `get_snapshot_meta`, `*_page`, …        |
//!
//! All items are re-exported at this level so every existing `logic::foo`
//! call site continues to compile without modification.

pub(crate) use snapshot::{
    get_guardians_page, get_reward_streams_page, get_snapshot, get_snapshot_meta, get_tasks_page,
    record_snapshot,
};
pub(crate) use vault_ops::{
    emergency_recover, lock_tokens, request_unlock, resign_guardian, unlock_tokens,
};
pub(crate) use voting::{process_vote, process_vote_batch, try_release_vault_funds, vote_inner};
