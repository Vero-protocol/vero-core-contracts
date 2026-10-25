//! Storage versioning and migration utilities with safety circuit breakers.
//!
//! The contract records a `StorageVersion` (u32) in instance storage so that
//! future schema upgrades can detect the current format and apply the
//! appropriate transformation steps.
//!
//! In order to prevent partial state corruption, this module implements a
//! "Commit-or-Rollback" pattern using an in-memory `MigrationCache` and performs
//! an atomic "Pre-Flight" validation check before committing state changes.

use crate::types::{ContractError, DataKey};
use soroban_sdk::{log, Address, BytesN, Env, IntoVal, Map, TryFromVal, Val, Vec};

/// The current on-chain storage schema version.
/// Increment this constant whenever the storage layout changes.
pub const CURRENT_VERSION: u32 = 1;

/// A temporary in-memory cache to hold state changes during a migration dry-run.
pub struct MigrationCache<'a> {
    env: &'a Env,
    updates: Map<DataKey, Val>,
    removals: soroban_sdk::Vec<DataKey>,
}

impl<'a> MigrationCache<'a> {
    /// Create a new migration cache.
    pub fn new(env: &'a Env) -> Self {
        Self {
            env,
            updates: Map::new(env),
            removals: soroban_sdk::Vec::new(env),
        }
    }

    /// Read a value, checking the temporary cache first.
    pub fn get<V>(&self, key: &DataKey) -> Option<V>
    where
        V: TryFromVal<Env, Val>,
    {
        if self.has_removal(key) {
            return None;
        }
        if let Some(val) = self.updates.get(key.clone()) {
            V::try_from_val(self.env, &val).ok()
        } else {
            self.env.storage().instance().get(key)
        }
    }

    /// Whether a key will exist after the staged updates and removals.
    /// Unlike `get`, this also detects a staged value with an invalid type.
    fn contains(&self, key: &DataKey) -> bool {
        if self.has_removal(key) {
            return false;
        }
        self.updates.contains_key(key.clone()) || self.env.storage().instance().has(key)
    }

    /// Write a value to the temporary cache.
    pub fn set<V>(&mut self, key: &DataKey, value: &V)
    where
        V: IntoVal<Env, Val>,
    {
        self.remove_from_removals(key);
        self.updates.set(key.clone(), value.into_val(self.env));
    }

    /// Remove a key from the temporary cache.
    #[allow(dead_code)]
    pub fn remove(&mut self, key: &DataKey) {
        self.updates.remove(key.clone());
        if !self.has_removal(key) {
            self.removals.push_back(key.clone());
        }
    }

    fn has_removal(&self, key: &DataKey) -> bool {
        for r in self.removals.iter() {
            if &r == key {
                return true;
            }
        }
        false
    }

    fn remove_from_removals(&mut self, key: &DataKey) {
        let mut new_removals = soroban_sdk::Vec::new(self.env);
        for r in self.removals.iter() {
            if &r != key {
                new_removals.push_back(r);
            }
        }
        self.removals = new_removals;
    }

    /// Commit all cached changes to the persistent instance storage.
    pub fn commit(&self) {
        for key in self.removals.iter() {
            self.env.storage().instance().remove(&key);
        }
        for (key, val) in self.updates.iter() {
            self.env.storage().instance().set(&key, &val);
        }
    }
}

/// Returns the storage version currently recorded on-chain.
/// Returns 0 if no version has been set (i.e. a pre-versioning contract).
pub fn get_version(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::StorageVersion)
        .unwrap_or(0)
}

/// Writes the given version into instance storage.
pub fn set_version(env: &Env, version: u32) {
    env.storage()
        .instance()
        .set(&DataKey::StorageVersion, &version);
}

/// Returns `true` if the on-chain storage schema is older than the
/// current binary's `CURRENT_VERSION`, meaning a migration is required.
#[allow(dead_code)]
pub fn needs_migration(env: &Env) -> bool {
    get_version(env) < CURRENT_VERSION
}

/// Run pre-flight checks to validate all contract invariants in the cache.
pub fn validate_migration(env: &Env, cache: &MigrationCache) -> Result<(), ContractError> {
    // 1. Verify StorageVersion is CURRENT_VERSION
    let version: u32 = cache.get(&DataKey::StorageVersion).unwrap_or(0);
    if version != CURRENT_VERSION {
        log!(
            env,
            "Validation failed: version mismatch. expected: {}, got: {}",
            CURRENT_VERSION,
            version
        );
        return Err(ContractError::InvalidVersion);
    }

    // 2. Validate Admin address if present in storage or cache
    if let Some(admin) = cache.get::<Address>(&DataKey::Admin) {
        if let Err(e) = crate::validation::validate_admin_address(env, &admin) {
            log!(env, "Validation failed: invalid admin address.", admin);
            return Err(e);
        }
    }

    // 3. Validate TokenAddress if present in storage or cache
    if let Some(token) = cache.get::<Address>(&DataKey::TokenAddress) {
        if let Err(e) = crate::validation::validate_external_address(env, &token) {
            log!(env, "Validation failed: invalid token address.", token);
            return Err(e);
        }
    }

    // 4. Validate VaultAddress if present
    if let Some(vault) = cache.get::<Address>(&DataKey::VaultAddress) {
        if let Err(e) = crate::validation::validate_external_address(env, &vault) {
            log!(env, "Validation failed: invalid vault address.", vault);
            return Err(e);
        }
    }

    // 5. Validate DripsAddress if present
    if let Some(drips) = cache.get::<Address>(&DataKey::DripsAddress) {
        if let Err(e) = crate::validation::validate_external_address(env, &drips) {
            log!(env, "Validation failed: invalid drips address.", drips);
            return Err(e);
        }
    }

    // 6. Validate WeightThreshold if present
    if let Some(threshold) = cache.get::<u64>(&DataKey::WeightThreshold) {
        if let Err(e) = crate::validation::validate_weight_threshold(threshold) {
            log!(
                env,
                "Validation failed: invalid weight threshold.",
                threshold
            );
            return Err(e);
        }
    }

    // 7. Validate FeeBps if present (must be <= 10000 bps)
    if let Some(fee_bps) = cache.get::<u32>(&DataKey::FeeBps) {
        if fee_bps > 10000 {
            log!(env, "Validation failed: invalid fee bps.", fee_bps);
            return Err(ContractError::InvalidConfig);
        }
    }

    // 8. Protect multi-signature upgrade configuration during migrations.
    // Check the effective staged state, including any unstaged keys already in
    // storage. The absence of both keys is legitimate before configuration.
    let has_signers = cache.contains(&DataKey::UpgradeSigners);
    let has_threshold = cache.contains(&DataKey::UpgradeThreshold);
    let upgrade_signers = if has_signers || has_threshold {
        let signers: Vec<Address> = cache
            .get(&DataKey::UpgradeSigners)
            .ok_or(ContractError::InvalidUpgradeConfig)?;
        let threshold: u32 = cache
            .get(&DataKey::UpgradeThreshold)
            .ok_or(ContractError::InvalidUpgradeConfig)?;

        if threshold == 0
            || threshold > signers.len()
            || !crate::utils::address::is_strictly_sorted_addresses(&signers)
        {
            log!(env, "Validation failed: invalid upgrade signer quorum.");
            return Err(ContractError::InvalidUpgradeConfig);
        }
        for signer in signers.iter() {
            crate::validation::validate_external_address(env, &signer)?;
        }
        Some(signers)
    } else {
        None
    };

    // 9. Pending proposals must be complete and use only authorized,
    // strictly sorted and unique approvals. A half-staged proposal could
    // otherwise become executable with an inconsistent quorum.
    let has_pending_wasm = cache.contains(&DataKey::PendingUpgradeWasm);
    let has_pending_approvals = cache.contains(&DataKey::PendingUpgradeApprovals);
    if has_pending_wasm || has_pending_approvals {
        let _: BytesN<32> = cache
            .get(&DataKey::PendingUpgradeWasm)
            .ok_or(ContractError::InvalidUpgradeConfig)?;
        let approvals: Vec<Address> = cache
            .get(&DataKey::PendingUpgradeApprovals)
            .ok_or(ContractError::InvalidUpgradeConfig)?;
        let signers = upgrade_signers.ok_or(ContractError::InvalidUpgradeConfig)?;

        if approvals.is_empty()
            || approvals.len() > signers.len()
            || !crate::utils::address::is_strictly_sorted_addresses(&approvals)
        {
            log!(env, "Validation failed: invalid pending upgrade approvals.");
            return Err(ContractError::InvalidUpgradeConfig);
        }
        for approver in approvals.iter() {
            if !signers.contains(approver) {
                log!(env, "Validation failed: unrecognized upgrade approver.");
                return Err(ContractError::InvalidUpgradeConfig);
            }
        }
    }

    Ok(())
}

/// Applies any pending storage migrations to bring the on-chain state
/// up to `CURRENT_VERSION`. The migration is **idempotent**: if the
/// storage is already at the latest version, this function returns
/// immediately without side-effects.
pub fn migrate(env: &Env) -> Result<(), ContractError> {
    let current = get_version(env);
    if current >= CURRENT_VERSION {
        return Ok(()); // already up to date
    }

    log!(
        env,
        "Starting atomic storage migration from version {} to {}",
        current,
        CURRENT_VERSION
    );

    // Initialize temporary cache
    let mut cache = MigrationCache::new(env);

    // ── v0 → v1 ──────────────────────────────────────────────────
    // Record CURRENT_VERSION in cache.
    cache.set(&DataKey::StorageVersion, &CURRENT_VERSION);

    // Future migrations (v1 → v2, etc.) will append transformation steps here, e.g.:
    // if current < 2 {
    //     // transform v1 keys to v2 format in cache
    // }

    // Run Pre-Flight check
    match validate_migration(env, &cache) {
        Ok(_) => {
            log!(
                env,
                "Migration pre-flight checks passed. Committing changes to storage."
            );
            cache.commit();
            Ok(())
        }
        Err(err) => {
            log!(
                env,
                "Migration FAILED at pre-flight check! Aborting and rolling back. Error: {:?}",
                err
            );
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn with_contract(run: impl FnOnce(&Env)) {
        let env = Env::default();
        let contract_id = env.register_contract(None, crate::VeroContract);
        env.as_contract(&contract_id, || run(&env));
    }

    fn prepared_cache(env: &Env) -> MigrationCache<'_> {
        let mut cache = MigrationCache::new(env);
        cache.set(&DataKey::StorageVersion, &CURRENT_VERSION);
        cache
    }

    fn sorted_signers(env: &Env) -> Vec<Address> {
        let first = Address::generate(env);
        let second = Address::generate(env);
        let (low, high) = if first < second {
            (first, second)
        } else {
            (second, first)
        };
        soroban_sdk::vec![env, low, high]
    }

    #[test]
    fn migration_accepts_unconfigured_and_valid_upgrade_quorums() {
        with_contract(|env| {
            let mut cache = prepared_cache(env);
            assert_eq!(validate_migration(env, &cache), Ok(()));

            let signers = sorted_signers(env);
            cache.set(&DataKey::UpgradeSigners, &signers);
            cache.set(&DataKey::UpgradeThreshold, &2u32);
            assert_eq!(validate_migration(env, &cache), Ok(()));
        });
    }

    #[test]
    fn migration_rejects_zero_excessive_and_missing_upgrade_thresholds() {
        with_contract(|env| {
            let mut cache = prepared_cache(env);
            let signers = sorted_signers(env);
            cache.set(&DataKey::UpgradeSigners, &signers);

            // A staged signer list without a matching threshold is invalid.
            assert_eq!(
                validate_migration(env, &cache),
                Err(ContractError::InvalidUpgradeConfig)
            );
            cache.set(&DataKey::UpgradeThreshold, &0u32);
            assert_eq!(
                validate_migration(env, &cache),
                Err(ContractError::InvalidUpgradeConfig)
            );
            cache.set(&DataKey::UpgradeThreshold, &3u32);
            assert_eq!(
                validate_migration(env, &cache),
                Err(ContractError::InvalidUpgradeConfig)
            );
            cache.set(&DataKey::UpgradeThreshold, &2u32);
            assert_eq!(validate_migration(env, &cache), Ok(()));

            cache.remove(&DataKey::UpgradeSigners);
            assert_eq!(
                validate_migration(env, &cache),
                Err(ContractError::InvalidUpgradeConfig)
            );
        });
    }

    #[test]
    fn migration_rejects_unsorted_or_duplicate_upgrade_signers() {
        with_contract(|env| {
            let mut cache = prepared_cache(env);
            let signers = sorted_signers(env);
            let low = signers.get(0).unwrap();
            let high = signers.get(1).unwrap();
            cache.set(&DataKey::UpgradeThreshold, &1u32);

            let unsorted = soroban_sdk::vec![env, high, low.clone()];
            cache.set(&DataKey::UpgradeSigners, &unsorted);
            assert_eq!(
                validate_migration(env, &cache),
                Err(ContractError::InvalidUpgradeConfig)
            );

            let duplicated = soroban_sdk::vec![env, low.clone(), low];
            cache.set(&DataKey::UpgradeSigners, &duplicated);
            assert_eq!(
                validate_migration(env, &cache),
                Err(ContractError::InvalidUpgradeConfig)
            );
        });
    }

    #[test]
    fn migration_validates_staged_threshold_against_existing_signers_without_committing() {
        with_contract(|env| {
            let signers = sorted_signers(env);
            env.storage()
                .instance()
                .set(&DataKey::UpgradeSigners, &signers);
            env.storage()
                .instance()
                .set(&DataKey::UpgradeThreshold, &2u32);

            let mut cache = prepared_cache(env);
            cache.set(&DataKey::UpgradeThreshold, &3u32);
            assert_eq!(
                validate_migration(env, &cache),
                Err(ContractError::InvalidUpgradeConfig)
            );
            assert_eq!(
                env.storage()
                    .instance()
                    .get::<_, u32>(&DataKey::UpgradeThreshold),
                Some(2),
            );

            cache.set(&DataKey::UpgradeThreshold, &1u32);
            assert_eq!(validate_migration(env, &cache), Ok(()));
        });
    }

    #[test]
    fn migration_requires_complete_pending_upgrade_proposals() {
        with_contract(|env| {
            let mut cache = prepared_cache(env);
            let signers = sorted_signers(env);
            cache.set(&DataKey::UpgradeSigners, &signers);
            cache.set(&DataKey::UpgradeThreshold, &2u32);
            let hash = BytesN::<32>::from_array(env, &[7u8; 32]);
            cache.set(&DataKey::PendingUpgradeWasm, &hash);

            // Proposal without approvals is incomplete.
            assert_eq!(
                validate_migration(env, &cache),
                Err(ContractError::InvalidUpgradeConfig)
            );
            let approvals = soroban_sdk::vec![env, signers.get(0).unwrap()];
            cache.set(&DataKey::PendingUpgradeApprovals, &approvals);
            assert_eq!(validate_migration(env, &cache), Ok(()));

            // Approvals without a proposal are equally invalid.
            cache.remove(&DataKey::PendingUpgradeWasm);
            assert_eq!(
                validate_migration(env, &cache),
                Err(ContractError::InvalidUpgradeConfig)
            );
        });
    }

    #[test]
    fn migration_rejects_unknown_or_duplicate_pending_approvers() {
        with_contract(|env| {
            let mut cache = prepared_cache(env);
            let signers = sorted_signers(env);
            cache.set(&DataKey::UpgradeSigners, &signers);
            cache.set(&DataKey::UpgradeThreshold, &1u32);
            cache.set(
                &DataKey::PendingUpgradeWasm,
                &BytesN::<32>::from_array(env, &[1u8; 32]),
            );

            let outsider = Address::generate(env);
            cache.set(
                &DataKey::PendingUpgradeApprovals,
                &soroban_sdk::vec![env, outsider],
            );
            assert_eq!(
                validate_migration(env, &cache),
                Err(ContractError::InvalidUpgradeConfig)
            );

            let signer = signers.get(0).unwrap();
            cache.set(
                &DataKey::PendingUpgradeApprovals,
                &soroban_sdk::vec![env, signer.clone(), signer],
            );
            assert_eq!(
                validate_migration(env, &cache),
                Err(ContractError::InvalidUpgradeConfig)
            );
        });
    }

    // Synthetic failing migration for future v1 -> v2 authors.
    //
    // This helper stages multiple storage mutations (an update, an addition,
    // and a removal) plus an invalid `FeeBps`, runs the same pre-flight
    // validation that real migrations use, and returns the validation error
    // WITHOUT committing. It is compiled only for unit-test builds and is
    // excluded from production builds via `#[cfg(test)]`.
    #[allow(dead_code)]
    fn synthetic_failing_v1_to_v2(env: &Env) -> Result<(), ContractError> {
        let mut cache = MigrationCache::new(env);

        // Stage a legitimate update to an existing key.
        cache.set(&DataKey::WeightThreshold, &1000u64);

        // Stage a legitimate addition of a brand-new key.
        cache.set(
            &DataKey::LockedBalance(env.current_contract_address()),
            &100i128,
        );

        // Stage a legitimate removal of an existing key.
        cache.remove(&DataKey::Task(1));

        // Inject an invalid FeeBps that must fail pre-flight validation.
        cache.set(&DataKey::FeeBps, &10001u32);

        // Pre-flight validation must fail before commit is ever reached.
        validate_migration(env, &cache)?;
        cache.commit();

        Ok(())
    }

    // Future migration authors: use `failing_multi_key_migration_rolls_back_all_staged_changes`
    // as the template for testing multi-key migrations. Stage updates, additions, and removals,
    // force validation to fail, and assert that real storage remains byte-for-byte unchanged.
    #[test]
    fn failing_multi_key_migration_rolls_back_all_staged_changes() {
        let env = Env::default();
        let contract_id = env.register_contract(None, crate::VeroContract);
        let contract_addr = contract_id.clone();

        env.as_contract(&contract_id, || {
            // ── Seed pre-migration on-chain state ─────────────────────
            env.storage()
                .instance()
                .set(&DataKey::StorageVersion, &CURRENT_VERSION);
            env.storage().instance().set(&DataKey::FeeBps, &25u32); // original valid fee
            env.storage()
                .instance()
                .set(&DataKey::WeightThreshold, &500u64); // to be updated
            env.storage().instance().set(&DataKey::Task(1), &7u64); // to be removed
                                                                    // DataKey::LockedBalance(contract) intentionally NOT seeded (new key).

            // ── Stage several changes, then force validation to fail ──
            let mut cache = MigrationCache::new(&env);
            cache.set(&DataKey::WeightThreshold, &1000u64);
            cache.set(&DataKey::LockedBalance(contract_addr.clone()), &100i128);
            cache.remove(&DataKey::Task(1));
            cache.set(&DataKey::FeeBps, &10001u32);

            // Staged state is present before validation (proves the test is not vacuous).
            assert_eq!(cache.get::<u64>(&DataKey::WeightThreshold), Some(1000));
            assert_eq!(
                cache.get::<i128>(&DataKey::LockedBalance(contract_addr.clone())),
                Some(100)
            );
            assert_eq!(cache.get::<u64>(&DataKey::Task(1)), None);
            assert_eq!(cache.get::<u32>(&DataKey::FeeBps), Some(10001));

            // Validation must fail before commit is reached.
            let result = validate_migration(&env, &cache);
            assert_eq!(result, Err(ContractError::InvalidConfig));

            // ── Real storage must remain completely unchanged ─────────
            // Updated key retains its original value.
            assert_eq!(
                env.storage()
                    .instance()
                    .get::<_, u64>(&DataKey::WeightThreshold),
                Some(500)
            );
            // New key remains absent.
            assert_eq!(
                env.storage()
                    .instance()
                    .get::<_, i128>(&DataKey::LockedBalance(contract_addr)),
                None
            );
            // Removed key is still present with its original value.
            assert_eq!(
                env.storage().instance().get::<_, u64>(&DataKey::Task(1)),
                Some(7)
            );
            // Original fee remains unchanged.
            assert_eq!(
                env.storage().instance().get::<_, u32>(&DataKey::FeeBps),
                Some(25)
            );
            // Migration version is not advanced.
            assert_eq!(get_version(&env), CURRENT_VERSION);
        });
    }
}
