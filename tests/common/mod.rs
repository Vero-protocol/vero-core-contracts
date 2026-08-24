use soroban_sdk::{contract, contractimpl, Address, Env};

/// Shared mock for the Drips protocol contract.
/// Used by both `purge_reward_stream` and `test` integration test suites
/// so the stub only needs to be maintained in one place (issue #199).
#[contract]
pub struct MockDripsContract;

#[contractimpl]
impl MockDripsContract {
    pub fn start_stream(
        _env: Env,
        _contributor: Address,
        _task_id: u64,
        _resolution_status: u32,
    ) {
    }
}
