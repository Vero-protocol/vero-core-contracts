use soroban_sdk::{contractclient, Env};

#[contractclient(name = "VaultClient")]
#[allow(dead_code)]
pub trait Vault {
    fn release_funds(env: Env, task_id: u64);
}
