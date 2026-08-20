#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as TestTokenClient};
use soroban_sdk::{Address, Env};
use vero_core_contracts::{Role, VeroContractClient};

fn setup() -> (Env, Address, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    client.initialize(&admin, &token, &100i128);
    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.grant_role(&admin, &admin, &Role::ConfigManager);

    (env, contract_id, admin, token, client)
}

#[test]
fn test_fee_deduction() {
    let (env, contract_id, admin, token, client) = setup();
    let guardian = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.add_guardian(&admin, &guardian);

    // Set fee and treasury
    client.set_treasury_address(&admin, &treasury);
    client.set_fee_bps(&admin, &1000); // 10% fee

    let sac = TestTokenClient::new(&env, &token);
    sac.mint(&guardian, &1000);

    // Lock tokens
    client.lock_tokens(&guardian, &1000);

    let token_client = TokenClient::new(&env, &token);

    // Guardian locks 1000. 10% fee = 100 goes to treasury. 900 locked.
    assert_eq!(token_client.balance(&treasury), 100);
    assert_eq!(token_client.balance(&contract_id), 900);
    assert_eq!(token_client.balance(&guardian), 0);

    // Request unlock and advance time
    client.request_unlock(&guardian);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 86401u64);

    // Unlock tokens
    client.resign_guardian(&guardian);

    // Unlocking 900. 10% fee = 90 goes to treasury. 810 to guardian.
    assert_eq!(token_client.balance(&treasury), 190);
    assert_eq!(token_client.balance(&contract_id), 0);
    assert_eq!(token_client.balance(&guardian), 810);
}
