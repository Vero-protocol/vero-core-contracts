#![allow(missing_docs)]

use soroban_sdk::{Address, Env, String, Vec};

use crate::types::ContractError;

/// The Stellar "zero" account address (G... strkey for an all-zero public key).
/// Passing this address to administrative functions must be rejected to avoid
/// black-holing permissions or funds.
pub const ZERO_ADDRESS_STR: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

/// Reject the Stellar zero-address.
///
/// Returns [`ContractError::InvalidAddress`] when `address` matches the
/// all-zero account strkey. This is the centralized sanitizer referenced in
/// issue #126.
pub fn validate_address(env: &Env, address: &Address) -> Result<(), ContractError> {
    let zero = String::from_str(env, ZERO_ADDRESS_STR);
    if address.to_string() == zero {
        return Err(ContractError::InvalidAddress);
    }
    Ok(())
}

/// Returns `true` iff `addrs` is strictly ordered, i.e. every address is
/// strictly greater than its predecessor (with no duplicates).
///
/// Empty and single-element lists are trivially sorted.
pub fn is_strictly_sorted_addresses(addrs: &Vec<Address>) -> bool {
    if addrs.len() < 2 {
        return true;
    }

    // SAFETY: this line is only reached when `addrs.len() >= 2`, so index 0
    // is always in range and `get(0)` is provably `Some`.
    let mut prev = addrs.get(0).unwrap();
    let mut i = 1;
    while i < addrs.len() {
        // SAFETY: the loop condition guarantees `i < addrs.len()`, so
        // `get(i)` is provably `Some`. Proven-safe invariant.
        let current = addrs.get(i).unwrap();
        if prev >= current {
            return false;
        }
        prev = current;
        i += 1;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn empty_and_single_element_lists_are_trivially_sorted() {
        let env = Env::default();
        assert!(is_strictly_sorted_addresses(&Vec::new(&env)));

        let single = soroban_sdk::vec![&env, Address::generate(&env)];
        assert!(is_strictly_sorted_addresses(&single));
    }

    #[test]
    fn strictly_sorted_list_passes() {
        let env = Env::default();
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let (low, high) = if a < b { (a, b) } else { (b, a) };
        let addrs = soroban_sdk::vec![&env, low, high];
        assert!(is_strictly_sorted_addresses(&addrs));
    }

    #[test]
    fn unsorted_list_is_rejected() {
        let env = Env::default();
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let (low, high) = if a < b { (a, b) } else { (b, a) };
        let addrs = soroban_sdk::vec![&env, high, low];
        assert!(!is_strictly_sorted_addresses(&addrs));
    }

    #[test]
    fn duplicate_addresses_are_rejected() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let addrs = soroban_sdk::vec![&env, addr.clone(), addr];
        assert!(!is_strictly_sorted_addresses(&addrs));
    }
}
