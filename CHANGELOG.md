# Changelog

All notable changes to the Vero Protocol core contracts will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial CHANGELOG.md to track protocol-level feature additions

### Changed
- N/A

### Deprecated
- N/A

### Removed
- N/A

### Fixed
- N/A

### Security
- N/A

---

## [0.1.0] - 2024-XX-XX

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

## [0.1.0-feat.69] - 2024-XX-XX

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

## Upcoming Features

The following features are visible in the source tree and will be documented when fully implemented:

### In Development
- **Fee-bps on Lock/Unlock**: Basis point fees for treasury operations
- **Multi-Sig Contract Upgrades**: Proxy upgrade pattern for contract governance
- **Storage Migrations**: Schema migration support
- **Task Archiving/Purging**: Cleanup of completed/failed tasks
- **Snapshots**: State snapshot functionality
- **Drips System**: Drip distribution mechanism
- **Consensus Module**: Core consensus logic
- **Events**: Event emission for contract actions

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
