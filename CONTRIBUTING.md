# Contributing to Vero Core Contracts

Thank you for your interest in contributing! This document covers everything you need to get started — from setting up your environment to opening a pull request.

---

## Table of Contents

- [Dev Environment Setup](#dev-environment-setup)
- [Build, Test, and Lint](#build-test-and-lint)
- [Branch and PR Conventions](#branch-and-pr-conventions)
- [Good First Issues](#good-first-issues)
- [Code of Conduct](#code-of-conduct)

---

## Dev Environment Setup

### Prerequisites

Install the Rust `wasm32` target (required to compile Soroban contracts):

```bash
rustup target add wasm32-unknown-unknown
```

Install the Soroban CLI:

```bash
cargo install --locked soroban-cli
```

### Clone and build

```bash
git clone https://github.com/your-org/vero-core-contracts.git
cd vero-core-contracts
cargo build --target wasm32-unknown-unknown --release
```

> **Tip:** A `.devcontainer` is available in the repo. Opening the project in a Dev Container (VS Code or GitHub Codespaces) will pre-install `soroban-cli` and the `wasm32-unknown-unknown` target automatically.

---

## Build, Test, and Lint

Use the Makefile targets for convenience:

| Command | What it does |
|---|---|
| `make build` | Compiles the contract to WASM (`--release`) |
| `make test` | Runs the full test suite |
| `make deploy-testnet` | Deploys to Stellar testnet (requires env vars) |

Or run the underlying Cargo commands directly:

```bash
# Build
cargo build --target wasm32-unknown-unknown --release

# Test
cargo test

# Lint — must pass with zero warnings
cargo clippy -- -D warnings

# Format — must be clean before opening a PR
cargo fmt --check
# To auto-fix formatting:
cargo fmt
```

All of these checks run automatically in CI on every pull request. Your PR will not be merged if any of them fail.

---

## Branch and PR Conventions

### Branch naming

Create branches off `main` using the pattern:

```
wave-N/<short-description>
```

Examples:
- `wave-1/configurable-vote-threshold`
- `wave-2/fix-ttl-drift`
- `wave-3/docs-inline-rustdoc`

### PR guidelines

- **One issue per PR.** Keep changes small and focused so reviewers can give useful feedback quickly.
- **Reference the issue number** in the PR title:
  ```
  fix: prevent duplicate guardian keys (#12)
  ```
- **All new code must have at least one test.** This applies to bug fixes too — add a regression test that would have caught the bug.
- **Run `cargo fmt` and `cargo clippy` locally** before pushing. Failing CI on formatting or lint warnings blocks the review.
- **Self-assign via `/claim`.** Comment `/claim` on an issue before starting work. Maintainers confirm within 24 hours to avoid duplicated effort.

### Commit messages

Use the conventional commits format:

```
<type>: <short summary> (#issue)
```

Common types: `feat`, `fix`, `docs`, `test`, `chore`, `refactor`.

---

## Good First Issues

New to the codebase? Look for issues labelled **`good first issue`** on the [Issues page](../../issues?q=is%3Aopen+label%3A%22good+first+issue%22).

These are scoped to be approachable without deep familiarity with the whole contract. Typical examples include:

- Adding `///` rustdoc comments to public functions
- Expanding the error-codes table in `README.md` with recovery steps
- Adding negative-path test cases (unregistered task vote, re-registration of an existing task ID)
- Improving the devcontainer or Makefile targets

If nothing in the list matches your interest, feel free to open an issue describing what you would like to work on and discuss the approach before starting.

---

## Code of Conduct

This project follows the [Contributor Covenant](https://www.contributor-covenant.org/version/2/1/code_of_conduct/) Code of Conduct. By participating you agree to abide by its terms. Please report unacceptable behaviour to the maintainers.

---

Apache-2.0 — contributions welcome.
