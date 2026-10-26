use crate::types::{ContractError, DataKey, WithdrawalRequest};
use soroban_sdk::{Address, Env};

/// Number of ledgers (~24 hours at 5 seconds per ledger) required before a
/// requested treasury withdrawal can be executed.
pub const TIME_LOCK_LEDGERS: u32 = 17_280;

/// Creates a new timelocked treasury withdrawal request and returns its ID.
pub fn request_withdrawal(
    env: &Env,
    admin: &Address,
    recipient: &Address,
    amount: i128,
) -> Result<u64, ContractError> {
    admin.require_auth();

    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(ContractError::NotInitialized)?;
    if admin != &stored_admin {
        return Err(ContractError::NotAuthorized);
    }

    let counter: u64 = env
        .storage()
        .instance()
        .get(&DataKey::WithdrawalCounter)
        .unwrap_or(0u64);
    let id = counter + 1;
    env.storage().instance().set(&DataKey::WithdrawalCounter, &id);

    let request = WithdrawalRequest {
        id,
        recipient: recipient.clone(),
        amount,
        requested_at_ledger: env.ledger().sequence(),
        is_executed: false,
        is_cancelled: false,
    };

    env.storage()
        .instance()
        .set(&DataKey::WithdrawalRequest(id), &request);

    crate::events::emit_withdrawal_requested(env, id, recipient, amount);

    Ok(id)
}

/// Executes a pending treasury withdrawal request after its timelock has elapsed.
pub fn execute_withdrawal(env: &Env, request_id: u64) -> Result<(), ContractError> {
    let mut request: WithdrawalRequest = env
        .storage()
        .instance()
        .get(&DataKey::WithdrawalRequest(request_id))
        .ok_or(ContractError::WithdrawalNotFound)?;

    if request.is_cancelled {
        return Err(ContractError::WithdrawalCancelled);
    }
    if request.is_executed {
        return Err(ContractError::WithdrawalAlreadyExecuted);
    }

    let current_ledger = env.ledger().sequence();
    if current_ledger < request.requested_at_ledger.saturating_add(TIME_LOCK_LEDGERS) {
        return Err(ContractError::TimeLockActive);
    }

    let token: Address = env
        .storage()
        .instance()
        .get(&DataKey::TokenAddress)
        .ok_or(ContractError::NotInitialized)?;
    let token_client = soroban_sdk::token::Client::new(env, &token);
    token_client.transfer(
        &env.current_contract_address(),
        &request.recipient,
        &request.amount,
    );

    request.is_executed = true;
    env.storage()
        .instance()
        .set(&DataKey::WithdrawalRequest(request_id), &request);

    crate::events::emit_withdrawal_executed(env, request_id, &request.recipient, request.amount);

    Ok(())
}

/// Cancels a pending treasury withdrawal request before execution.
pub fn cancel_withdrawal(
    env: &Env,
    admin: &Address,
    request_id: u64,
) -> Result<(), ContractError> {
    admin.require_auth();

    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(ContractError::NotInitialized)?;
    if admin != &stored_admin {
        return Err(ContractError::NotAuthorized);
    }

    let mut request: WithdrawalRequest = env
        .storage()
        .instance()
        .get(&DataKey::WithdrawalRequest(request_id))
        .ok_or(ContractError::WithdrawalNotFound)?;

    if request.is_executed {
        return Err(ContractError::WithdrawalAlreadyExecuted);
    }
    if request.is_cancelled {
        return Err(ContractError::WithdrawalCancelled);
    }

    request.is_cancelled = true;
    env.storage()
        .instance()
        .set(&DataKey::WithdrawalRequest(request_id), &request);

    crate::events::emit_withdrawal_cancelled(env, request_id);

    Ok(())
}

/// Retrieves a treasury withdrawal request by ID, if it exists.
pub fn get_withdrawal_request(env: &Env, request_id: u64) -> Option<WithdrawalRequest> {
    env.storage()
        .instance()
        .get(&DataKey::WithdrawalRequest(request_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VeroContract, VeroContractClient};
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};

    fn setup() -> (Env, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, VeroContract);
        let client = VeroContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let token_addr = token.address();

        client.initialize(&admin, &token_addr, &100i128);
        StellarAssetClient::new(&env, &token_addr).mint(&contract_id, &5_000i128);

        (env, contract_id, admin, token_addr)
    }

    #[test]
    fn request_and_execute_withdrawal_after_timelock() {
        let (env, contract_id, admin, token_addr) = setup();
        let recipient = Address::generate(&env);

        let id = env.as_contract(&contract_id, || {
            request_withdrawal(&env, &admin, &recipient, 1_200).unwrap()
        });
        assert_eq!(id, 1);

        // Before timelock elapses, execution must fail with TimeLockActive.
        let early = env.as_contract(&contract_id, || execute_withdrawal(&env, id));
        assert_eq!(early, Err(ContractError::TimeLockActive));

        // Advance ledger sequence past TIME_LOCK_LEDGERS and execute.
        env.ledger().set_sequence_number(TIME_LOCK_LEDGERS + 10);
        env.as_contract(&contract_id, || {
            execute_withdrawal(&env, id).unwrap();
        });

        let stored = env.as_contract(&contract_id, || get_withdrawal_request(&env, id).unwrap());
        assert!(stored.is_executed);
        assert!(!stored.is_cancelled);
        assert_eq!(TokenClient::new(&env, &token_addr).balance(&recipient), 1_200);

        // Second execution attempt must fail with WithdrawalAlreadyExecuted.
        let duplicate = env.as_contract(&contract_id, || execute_withdrawal(&env, id));
        assert_eq!(duplicate, Err(ContractError::WithdrawalAlreadyExecuted));
    }

    #[test]
    fn cancel_withdrawal_blocks_execution_and_double_cancel() {
        let (env, contract_id, admin, _token_addr) = setup();
        let recipient = Address::generate(&env);
        let outsider = Address::generate(&env);

        let id = env.as_contract(&contract_id, || {
            request_withdrawal(&env, &admin, &recipient, 500).unwrap()
        });

        // Non-admin cannot cancel.
        let unauthorized = env.as_contract(&contract_id, || cancel_withdrawal(&env, &outsider, id));
        assert_eq!(unauthorized, Err(ContractError::NotAuthorized));

        env.as_contract(&contract_id, || {
            cancel_withdrawal(&env, &admin, id).unwrap();
        });

        let duplicate_cancel =
            env.as_contract(&contract_id, || cancel_withdrawal(&env, &admin, id));
        assert_eq!(duplicate_cancel, Err(ContractError::WithdrawalCancelled));

        env.ledger().set_sequence_number(TIME_LOCK_LEDGERS + 10);
        let exec_cancelled = env.as_contract(&contract_id, || execute_withdrawal(&env, id));
        assert_eq!(exec_cancelled, Err(ContractError::WithdrawalCancelled));
    }

    #[test]
    fn missing_withdrawal_returns_not_found() {
        let (env, contract_id, admin, _token_addr) = setup();

        let exec_missing = env.as_contract(&contract_id, || execute_withdrawal(&env, 999));
        assert_eq!(exec_missing, Err(ContractError::WithdrawalNotFound));

        let cancel_missing =
            env.as_contract(&contract_id, || cancel_withdrawal(&env, &admin, 999));
        assert_eq!(cancel_missing, Err(ContractError::WithdrawalNotFound));
    }
}
