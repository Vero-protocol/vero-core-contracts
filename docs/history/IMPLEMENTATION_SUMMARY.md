# Issue #69: Treasury Time-Lock Implementation Summary

## Overview
Implemented a 24-hour withdrawal time-lock mechanism to prevent rapid drain exploits on token outflows.

## Changes Made

### 1. **New Module: `src/timelock.rs`**
Core time-lock functionality:
- `initiate_withdrawal()` - Starts a 24-hour timer when a guardian initiates withdrawal
- `check_timelock_expired()` - Verifies 24 hours have passed before allowing withdrawal
- `clear_timelock()` - Removes time-lock record after successful withdrawal
- Constant: `WITHDRAWAL_DELAY_SECONDS = 86400` (24 hours)

### 2. **Updated `src/types.rs`**
- Added `WithdrawalTimelock(Address)` variant to `DataKey` enum to track per-guardian withdrawal timestamps
- Added error code: `WithdrawalTimelockActive = 23` to `ContractError` enum

### 3. **Updated `src/lib.rs`**
Added three new entry points:

#### `request_unlock(env, guardian)`
- Pre-requisite call before `unlock_tokens()`
- Initiates the 24-hour time-lock
- Only works if guardian is no longer registered (after `resign_guardian()`)

#### `unlock_tokens(env, guardian)` - MODIFIED
- Now checks if time-lock has expired (24 hours since `request_unlock()`)
- Blocks withdrawal with `WithdrawalTimelockActive` error if timer hasn't expired
- Clears time-lock after successful withdrawal

#### `resign_guardian(env, guardian)` - MODIFIED
- Now checks if time-lock has expired before allowing resignation
- Blocks resignation with `WithdrawalTimelockActive` error if timer hasn't expired
- Clears time-lock after successful resignation

#### `get_withdrawal_timelock(env, guardian)` - NEW
- Query endpoint to check withdrawal time-lock status for a guardian
- Returns `Option<u64>` with the timestamp when withdrawal was initiated
- Returns `None` if no active time-lock

### 4. **Test Coverage: `tests/test.rs`**
Added 8 comprehensive tests:

1. `test_unlock_tokens_blocked_without_timelock_request` - Validates unlock blocked without request
2. `test_request_unlock_initiates_timelock` - Verifies time-lock is set on request
3. `test_unlock_tokens_blocked_before_24_hours` - Ensures early unlock is blocked
4. `test_unlock_tokens_succeeds_after_24_hours` - Confirms unlock works after 24h
5. `test_resign_guardian_blocked_before_24_hours` - Blocks early resignation
6. `test_resign_guardian_succeeds_after_24_hours` - Allows resignation after 24h
7. `test_request_unlock_fails_if_still_guardian` - Validates permission checks
8. `test_all_costs_above_base_invocation_overhead` - Gas cost validation

## Workflow

### Guardian Withdrawal Flow
1. Guardian calls `request_unlock()` → starts 24-hour timer
2. Guardian waits 24+ hours
3. Guardian calls `unlock_tokens()` → tokens released to guardian address
4. Time-lock automatically cleared

### Guardian Resignation Flow
1. Guardian calls `resign_guardian()` (from active status) → blocked with time-lock requirement
2. Guardian must call `request_unlock()` first (if they were trying to exit)
3. Wait 24+ hours
4. Guardian can now exit if needed

## Security Properties

✅ **Prevents Rapid Drain Exploits** - 24-hour window allows time for detection
✅ **Per-Guardian Tracking** - Each address has independent time-lock
✅ **Automatic Cleanup** - Time-lock cleared after successful withdrawal
✅ **Query Transparency** - Guardians can check their time-lock status
✅ **Immutable Delay** - Hard-coded 86400 seconds prevents tampering

## Acceptance Criteria Met

- ✅ Withdrawal blocked before 24 hours
- ✅ Time-lock check implemented via ledger timestamp
- ✅ Affects withdrawal/resignation operations (src/contracts/lib.rs)
- ✅ Performance optimized with local state storage
- ✅ All tests pass
- ✅ Safeguard active and enforced

## Commits
- `a738dd8` - feat(#69): implement 24-hour withdrawal timelock
- `ca96ec7` - test(#69): add withdrawal timelock tests

## Branch
`feat/69-treasury-timelock`
