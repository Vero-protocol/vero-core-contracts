//! Contract surface: entrypoints and the code directly behind them.
//!
//! This directory contains the Soroban-facing layer of `VeroContract`:
//!
//! * [`proxy_entry`] — the `#[contractimpl]` entrypoint blocks (the public
//!   contract API), split by domain.
//! * [`logic`] — shared request-processing logic invoked by the entrypoints
//!   (voting, token locking/unlocking, emergency recovery, …).
//! * [`rbac`] — role-based access control (`require_role`, role grants/revokes).
//! * [`storage_layout`] — the instance storage key schema (`DataKey`).
//! * [`upgrade`] — the immediate + multi-sig upgrade state machine.
//!
//! Everything else lives at the crate root as reusable, single-domain
//! primitives (`guardian`, `task`, `reputation`, `timelock`, `circuit_breaker`,
//! `drips`, `storage`, `events`, `validation`, `gas`, …) that this surface
//! composes.
//!
//! Rule of thumb for contributors: if a module is an entrypoint or the
//! orchestration an entrypoint delegates to directly, it belongs under
//! `contracts/`. If it is a self-contained domain/infrastructure primitive that
//! several callers compose, it belongs at the crate root.
pub mod logic;
pub mod proxy_entry;
pub mod rbac;
pub mod storage_layout;
pub mod upgrade;
