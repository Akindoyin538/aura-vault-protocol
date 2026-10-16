#![cfg(test)]

// The fuzz module is disabled — the original code used APIs that are
// incompatible with Soroban SDK 22 (MockAuthorizationContext, Address::from_contract_id
// with wrong signature, direct contract function calls instead of client).
// The property-based tests in test.rs cover the same invariants using the
// correct testutils API.
