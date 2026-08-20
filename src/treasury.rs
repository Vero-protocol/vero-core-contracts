use crate::types::{ContractError, DataKey, WithdrawalRequest};
use soroban_sdk::{Address, Env};

// ~24 hours at Stellar's target of 5 seconds per ledger
pub const TIME_LOCK_LEDGERS: u32 = 17_280;

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

pub fn get_withdrawal_request(env: &Env, request_id: u64) -> Option<WithdrawalRequest> {
    env.storage()
        .instance()
        .get(&DataKey::WithdrawalRequest(request_id))
}
