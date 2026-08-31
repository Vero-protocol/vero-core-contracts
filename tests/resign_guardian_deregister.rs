use soroban_sdk::token::StellarAssetClient as TestTokenClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};
use vero_core_contracts::VeroContractClient;
use vero_core_contracts::Role;

fn setup() -> (Env, Address, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = token.address();

    client.initialize(&admin, &token_addr, &100i128);

    (env, contract_id, admin, token_addr, client)
}

fn lock_for_guardian(
    env: &Env,
    token: &Address,
    client: &VeroContractClient,
    guardian: &Address,
    amount: i128,
) {
    let sac = TestTokenClient::new(env, token);
    sac.mint(guardian, &amount);
    client.lock_tokens(guardian, &amount);
}

#[test]
fn test_resigned_guardian_can_be_readded() {
    let (env, _contract_id, admin, token, client) = setup();
    let g = Address::generate(&env);

    client.grant_role(&admin, &admin, &Role::GuardianManager);
    client.add_guardian(&admin, &g);
    lock_for_guardian(&env, &token, &client, &g, 200);

    // Initiate the 24-hour timelock, then advance ledger past it
    client.request_unlock(&g);
    let timelock = client.get_withdrawal_timelock(&g).unwrap();
    env.ledger().set_timestamp(timelock + 86401u64);

    client.resign_guardian(&g);

    // Verify guardian is deregistered from all structures
    assert!(!client.is_guardian(&g));
    let snapshot_meta = client.get_snapshot_meta();
    assert_eq!(snapshot_meta.guardian_count, 0);
    
    let guardians_page = client.get_guardians_page(&0u32, &50u32);
    assert_eq!(guardians_page.len(), 0);

    // Re-add the guardian - should succeed after deregistration
    client.add_guardian(&admin, &g);
    assert!(client.is_guardian(&g));
    
    let snapshot_meta_after = client.get_snapshot_meta();
    assert_eq!(snapshot_meta_after.guardian_count, 1);
    
    let guardians_page_after = client.get_guardians_page(&0u32, &50u32);
    assert_eq!(guardians_page_after.len(), 1);
}
