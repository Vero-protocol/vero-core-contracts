//! Single home for every protocol size/limit constant.
//!
//! Tuning a bound (e.g. for a gas-budget adjustment) and auditing that all
//! limits are internally consistent means looking in exactly one place. See
//! issue #227 — these constants were previously scattered across
//! `contracts/logic.rs`, `task.rs`, and `validation.rs`.

/// Maximum batch size for a single `register_task` call.
pub const MAX_REGISTER_TASK_BATCH_SIZE: u32 = 32;

/// Maximum number of entries `get_snapshot`/`record_snapshot` will read from
/// any single tracked collection (guardians, tasks, reward streams) before
/// refusing to build a snapshot.
///
/// Building a full snapshot costs roughly 2 storage reads per guardian
/// (guardian flag + reputation), 2+ reads per task (task struct + its voter
/// list), and 2 reads per reward stream. At `MAX_SNAPSHOT_COLLECTION_SIZE`
/// entries per collection that stays comfortably inside Soroban's
/// per-transaction CPU instruction budget with wide margin — see the
/// growth-curve measurements in `tests/snapshot_scaling.rs`. Once a
/// collection approaches this ceiling, callers should switch to the
/// paginated API (`get_snapshot_meta` + `get_guardians_page` +
/// `get_tasks_page` + `get_reward_streams_page`), which reads at most
/// `O(limit)` entries per call — not `O(total collection size)` — and stays
/// cheaply invokable well past the point this cap would refuse to build a
/// full snapshot.
pub const MAX_SNAPSHOT_COLLECTION_SIZE: u32 = 200;

/// Maximum number of entries any paginated snapshot call will return,
/// regardless of the caller-requested `limit`. Keeps a single page call's
/// cost bounded even against a hostile/misconfigured caller.
pub const MAX_PAGE_LIMIT: u32 = 50;

/// Maximum allowed task id (`u64::MAX / 2`), so ids stay comfortably below
/// `u64::MAX` and away from overflow-prone math elsewhere.
pub const MAX_TASK_ID: u64 = u64::MAX / 2;

/// Upper bound for token amounts locked/transferred by the contract.
pub const MAX_TOKEN_AMOUNT: i128 = i128::MAX / 2;

/// Upper bound for the vote-lock threshold, one below `MAX_TOKEN_AMOUNT`.
pub const MAX_LOCK_THRESHOLD: i128 = MAX_TOKEN_AMOUNT - 1;

/// Upper bound for a single guardian's reputation score.
pub const MAX_REPUTATION_SCORE: u64 = 1_000_000_000;

/// Upper bound for the cumulative weight required to resolve a task. Enforced by both
/// the live setter (`set_weight_threshold`) and migration pre-flight checks (`validate_migration`).
pub const MAX_WEIGHT_THRESHOLD: u64 = 1_000_000_000_000;
