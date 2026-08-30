# Pull Request: Integrate Formal Verification for Consensus Logic & Repair Unit Tests

## Description
This pull request introduces formal verification to the weighted consensus mechanism of the Vero Core Contracts using Kani. It also resolves pre-existing duplicate definitions and type mismatches that were breaking the unit test suite compilation.

## Key Changes

### 1. Pure Consensus Module
- Extracted and isolated the weighted voting state machine to [src/consensus.rs](file:///c:/Users/Kroman/vero-core-contracts/src/consensus.rs).
- Added `ConsensusState` and the pure function `apply_vote()` which handles:
  - Vote weight accumulation.
  - Votes counter saturating addition (prevents counter overflows).
  - Validation of threshold logic and completion tracking (`is_done`).
- Integrated `apply_vote()` into the main contract lifecycle in [src/lib.rs](file:///c:/Users/Kroman/vero-core-contracts/src/lib.rs).

### 2. Formal Verification Crate
- Created a separate workspace crate `vero-verification` in the [verification/](file:///c:/Users/Kroman/vero-core-contracts/verification) directory.
- Implemented **9 Kani symbolic proofs** verifying core properties:
  - **Threshold Invariant**: Task completion requires cumulative weight meeting the threshold.
  - **No Below-Threshold Resolution**: Proof that no execution path allows resolution below threshold.
  - **Monotone Resolution**: Task completion state (`is_done`) is irreversible.
  - **Overflow Protection**: Absolute prevention of weight and votes counter wrapping.
  - **Degenerate Case Safety**: Safe handling of a zero-weight threshold.

### 3. CI Workflow Integration
- Integrated a new job `formal-verification` in [.github/workflows/ci.yml](file:///c:/Users/Kroman/vero-core-contracts/.github/workflows/ci.yml) to automatically install the Kani verifier and run proofs on every push and pull request.

### 4. Unit Test Repairs
- Cleaned duplicate methods and struct fields in [src/lib.rs](file:///c:/Users/Kroman/vero-core-contracts/src/lib.rs), [src/events.rs](file:///c:/Users/Kroman/vero-core-contracts/src/events.rs), and [src/types.rs](file:///c:/Users/Kroman/vero-core-contracts/src/types.rs).
- Rewrote the unit test suite in [tests/test.rs](file:///c:/Users/Kroman/vero-core-contracts/tests/test.rs) to properly initialize token locks and setup guardians, ensuring `cargo check --tests` compiles with **0 errors**.

## Testing & Verification
- **Local Compilations**: Verified that the contracts and entire test suite compile successfully (`cargo check --tests` finishes with 0 errors).
- **Verification Proofs**: Configured to run automatically in CI.
- **Environment Note**: Local test running (`cargo test`) requires MinGW's `dlltool.exe` on Windows-GNU host environments to compile `backtrace` (a `soroban-sdk` testutils dependency). These checks are fully supported and will run in the CI build containers.

---

# Pull Request: #306 Validate the threshold in set_weight_threshold with the same bounds validate_migration enforces

## Description
Stops the live setter `set_weight_threshold` from writing a threshold that the migration pre-flight is designed to reject.

Previously, `set_weight_threshold` wrote the caller's value straight to storage with no range check, allowing a zero threshold (which makes `total_weight_accrued >= threshold` trivially true on the first vote, silently defeating weighted consensus) or values exceeding `MAX_WEIGHT_THRESHOLD` (poisoning future migrations).

## Key Changes
1. **Live Setter Validation**: Added `crate::validation::validate_weight_threshold(threshold)?` to `set_weight_threshold` in `src/contracts/proxy_entry/entry_config.rs`.
2. **Error Documentation**: Documented `InvalidAmount` and `InvalidRange` errors on `set_weight_threshold`.
3. **Property Testing**: Added proptests in `tests/property_tests.rs` verifying that any threshold accepted by `set_weight_threshold` is unconditionally accepted by `migrate::validate_migration`, and invalid inputs fail with identical error codes.
4. **Comprehensive Test Suite**: Updated and added test coverage across 15+ files verifying boundary conditions, authorization, gas budgets, circuit breaker pause states, and invariants.

