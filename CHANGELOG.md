# Changelog

All notable changes to the Vero Protocol core contracts will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Role-Based Access Control**: `Role` enum with admin/manager/guardian permissions enforced across privileged entry points (#110)
- **Custom Contract Errors**: typed `ContractError` enum with `panic_with_error!` and centralized error variants (#96, #111)
- **Contract-Level Pause**: `require_not_paused` extracted into a dedicated `guards` module and applied to privileged entry points (#114, #117)
- **Emergency Stop / Recovery**: emergency stop circuit for fund operations, emergency recovery mode, and `emergency_recover` entry point bypassing the pause gate (#99, #116, #172)
- **Multi-Sig Contract Upgrades**: enforce multi-sig quorum with sorted approvals before contract upgrades (#109, #118)
- **Protocol Event Emission**: emit events for state changes, optimized with a compact bitmask format (#102, #120)
- **Storage Versioning**: versioned contract state storage with atomic migration pre-flight checks and commit-or-rollback cache (#113, #130)
- **Dynamic Consensus Quorum**: per-task `min_votes_required` for a configurable quorum (#112)
- **Protocol Fees**: configurable fee-bps deducted from token movements (#123)
- **Guardian Rotation**: mechanism to rotate guardians (#124)
- **Zero-Address Validation**: reject zero-address administrative inputs (#129)
- **Batch Operations**: atomic batch voting and batch contract calls for transaction aggregation (#93, #98)
- **Decoupled Logic/Storage**: native proxy pattern separating contract logic from storage (#92)
- **Task Purging**: `purge_task` removes terminal tasks (including reward streams) from storage (#95, #180)
- **Task Archiving RBAC**: `archive_task` now requires the TaskManager role (#179)
- **Bounded Snapshot API**: `get_snapshot`/`record_snapshot` are cost-bounded with a paginated snapshot metadata API (#182)
- **Formal Verification**: full K-framework formal verification setup for consensus invariants (#108)
- **Testing**: gas-budget assertions for high-traffic operations, proptest property tests for consensus invariants, task-resolution regression test, and end-to-end happy-path integration test (#181, #187, #259, #188)
- **Issue Templates**: bug report, feature request, and good-first-issue templates (#253)
- **Batch Execution Cost Bound**: `batch_execute` rejects batches whose summed estimated instruction cost exceeds `MAX_BATCH_EXECUTE_COST` with `BatchTooLarge` (#214)

### Changed
- **Per-Task Vote Storage**: track voters per-task instead of a global vote list to improve scaling (#91)
- Remove unused dependencies (#131)
- Deduplicate `ZERO_ADDRESS_STR` constant between `src` and tests (#233)
- Re-export `ARCHIVE_AFTER_SECONDS` and remove duplicate tests (#257)
- Extract upgrade logic from `proxy_entry.rs` into `contracts::upgrade` (#260)
- Move consensus inline tests to `tests/consensus.rs` (#235)
- Validate each address exactly once per call path (#236)
- Add `initialize-testnet` to the Makefile header comment (#234)
- Wire `proofs/build.ps1` as the documented Windows entry point (#237)
- Reconcile `BatchCall` and `Operation` enums into a single source of truth via `BatchCall::operation()`, add the missing `Operation` variants (`CancelTask`, `RemoveGuardian`, `RequestUnlock`, `SetVaultAddress`, `Pause`, `Unpause`), and give `gas::get_estimated_cost` coverage for every batchable operation; recalibrate gas constants against measured CPU instruction costs (#214)
- Document the `contracts/` vs. crate-root module boundary in `contracts/mod.rs` and `lib.rs` (#210)
- Docs: README module registry, DataKey listing, and error-codes table synced with `src/`; duplicate Modules table and emergency-halt procedure removed; missing `grant_role` calls added to Quick Start; rustdoc added to `rbac.rs`, `storage_layout.rs`, and `validation.rs`; API documentation formalized; `DEPLOY.md`, `CONTRIBUTING.md`, and `TODO.md` added; `Description.md` purpose documented for the GrantFox registry; gas-benchmarks reference and reproduction instructions added; changelog release dates replaced with actual merge dates (#115, #173, #174, #177, #178, #183, #186, #189, #190, #251, #252, #254, #255, #258)

### Deprecated
- N/A

### Removed
- N/A

### Fixed
- `resolved`/`wt_vote` events no longer truncate `task_id`/`weight` to 32 bits; both are now emitted as full `u64` values (#159)
- `record_failure` is no longer permissionless and is now rate-limited (#175)
- Isolate vault `release_funds` call to prevent task-resolution DoS (#184)
- Sanitize administrative inputs and validate input ranges (#94, #97)
- Fix compile errors and missing `testutils::Address` import after fee changes (#123)
- Add end-to-end happy-path integration test for #137 (#188)

### Security
- CI build/test workflow and dependency security scanning added (#176)

---

## [0.1.0] - 2026-05-18

### Added
- **Core Contract**: Initial VeroContract deployment for GitHub PR verification on Soroban
- **Guardian System**: Whitelist trusted validators with reputation scores
- **Task Management**: Register and track GitHub PRs as tasks with unique IDs
- **Voting Mechanism**: Guardians vote on tasks with reputation-weighted votes
- **Reputation System**: Track guardian reputation scores for weighted voting
- **Circuit Breaker**: Auto-pause after >50 failures with admin reset
- **Pause/Unpause**: Admin-controlled emergency pause functionality
- **Storage Layout**: DataKey enum for instance storage

### Changed
- N/A (initial release)

### Security
- Zero address validation for guardian addresses
- Reentrancy protection

---

## [0.1.0-feat.69] - 2026-06-18

### Added
- **24-Hour Withdrawal Timelock**: Implemented time-lock mechanism to prevent rapid drain exploits
  - `request_unlock()` - Initiates 24-hour timer for guardian withdrawal
  - `unlock_tokens()` - Requires 24-hour wait before token release
  - `resign_guardian()` - Blocks resignation until timelock expires
  - `get_withdrawal_timelock()` - Query endpoint for timelock status
- **WithdrawalTimelock DataKey**: New variant for per-guardian withdrawal timestamps
- **WithdrawalTimelockActive Error**: Error code 23 for blocked operations

### Security
- Prevents rapid drain exploits with 24-hour delay
- Per-guardian independent timelock tracking

---

## Changelog Guidelines

### For Contributors

When submitting a PR that introduces changes to the protocol:

1. **Add an entry** to the [Unreleased] section at the top of this file
2. **Use the appropriate category**: Added, Changed, Deprecated, Removed, Fixed, Security
3. **Be descriptive**: Explain what changed and why it matters
4. **Reference files**: Include relevant file paths when applicable
5. **Reference issues**: Include issue numbers when applicable (e.g., `Closes #123`)

### Versioning

This project follows [Semantic Versioning](https://semver.org/):
- **MAJOR** version for incompatible API changes
- **MINOR** version for backwards-compatible functionality additions
- **PATCH** version for backwards-compatible bug fixes

---

## Footer

This CHANGELOG is maintained by the Vero Protocol team and contributors.

For the full commit history, see the [GitHub repository](https://github.com/Vero-protocol/vero-core-contracts).
