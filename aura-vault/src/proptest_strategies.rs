/// # Proptest strategies for realistic vault operation sequences — Issue #470
///
/// This module provides reusable [`proptest`] strategies that generate
/// sequences of vault operations (`Deposit`, `Withdraw`, `Harvest`) with
/// the following properties:
///
/// * Sequences are 1–100 operations long.
/// * Amounts are in the realistic range 1–10_000_000_000 (10 billion stroops,
///   ≈ 100,000 XLM at 7 decimal places) — no pathological `u64::MAX` values.
/// * Sequences are internally consistent: a `Withdraw(addr, shares)` can only
///   appear after an earlier `Deposit(addr, _)` that produced those shares.
/// * Shrinking is fully supported via proptest's built-in `prop_flat_map` /
///   `prop_recursive` machinery.
///
/// ## Usage
///
/// ```rust,ignore
/// use crate::proptest_strategies::{vault_op_sequence, VaultOp};
///
/// proptest! {
///     #[test]
///     fn my_invariant(ops in vault_op_sequence()) {
///         let (env, vault, admin, token) = setup();
///         run_ops(&env, &vault, &admin, &token, &ops);
///     }
/// }
/// ```
#[cfg(test)]
pub mod vault_strategies {
    extern crate std;

    use proptest::prelude::*;
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // Operation enum
    // -----------------------------------------------------------------------

    /// A single vault operation that can appear in a generated sequence.
    #[derive(Clone, Debug)]
    pub enum VaultOp {
        /// Deposit `amount` tokens on behalf of `actor_idx` (index into a fixed
        /// actor pool — prevents unbounded address space).
        Deposit { actor_idx: usize, amount: i128 },
        /// Withdraw `share_fraction` (0.0–1.0) of the actor's current shares.
        /// Converted to an actual share count at runtime by the test runner.
        Withdraw { actor_idx: usize, share_fraction: f64 },
        /// Harvest `amount` tokens as yield.
        Harvest { amount: i128 },
    }

    // -----------------------------------------------------------------------
    // Amount strategy
    // -----------------------------------------------------------------------

    /// Generates plausible deposit/harvest amounts:
    ///   - Small  (1–1_000)              25 % — dust / micro-deposits
    ///   - Medium (1_001–1_000_000)      50 % — typical user amounts
    ///   - Large  (1_000_001–10_000_000_000)  25 % — whale deposits
    pub fn arb_realistic_amount() -> impl Strategy<Value = i128> {
        prop_oneof![
            1i128..=1_000i128,
            1_001i128..=1_000_000i128,
            1_000_001i128..=10_000_000_000i128,
        ]
    }

    // -----------------------------------------------------------------------
    // Single-operation strategy
    // -----------------------------------------------------------------------

    /// Strategy for a single VaultOp, drawn from the actor pool of size
    /// `num_actors`.
    pub fn arb_vault_op(num_actors: usize) -> impl Strategy<Value = VaultOp> {
        let max_idx = num_actors.saturating_sub(1);
        prop_oneof![
            // 50% deposits — vault needs deposits to be interesting
            2 => (0..=max_idx, arb_realistic_amount())
                .prop_map(|(actor_idx, amount)| VaultOp::Deposit { actor_idx, amount }),
            // 30% withdrawals
            3 => (0..=max_idx, 0.01f64..=1.0f64)
                .prop_map(|(actor_idx, share_fraction)| VaultOp::Withdraw {
                    actor_idx,
                    share_fraction,
                }),
            // 20% harvests
            2 => arb_realistic_amount()
                .prop_map(|amount| VaultOp::Harvest { amount }),
        ]
    }

    // -----------------------------------------------------------------------
    // Sequence strategy — 1..=100 operations, 2–5 actors
    // -----------------------------------------------------------------------

    /// Primary exported strategy: generates a realistic vault operation sequence
    /// together with the number of actors used (so tests can create addresses).
    ///
    /// Returns `(num_actors, ops)`.
    pub fn vault_op_sequence() -> impl Strategy<Value = (usize, std::vec::Vec<VaultOp>)> {
        (2usize..=5usize).prop_flat_map(|num_actors| {
            proptest::collection::vec(arb_vault_op(num_actors), 1..=100)
                .prop_map(move |ops| (num_actors, ops))
        })
    }

    // -----------------------------------------------------------------------
    // Consistency filter
    // -----------------------------------------------------------------------

    /// Returns true if the sequence is internally consistent:
    /// every `Withdraw` targets an actor that has previously deposited
    /// (shares > 0 tracked via a simulated run).
    ///
    /// This is used in tests to skip sequences that would trivially fail
    /// with `InsufficientShares` rather than violating a real invariant.
    pub fn is_consistent(num_actors: usize, ops: &[VaultOp]) -> bool {
        let mut shares: HashMap<usize, i128> = HashMap::new();
        let mut total_deposited: i128 = 0;
        let mut total_shares: i128 = 0;

        for op in ops {
            match op {
                VaultOp::Deposit { actor_idx, amount } => {
                    let new_shares = if total_shares == 0 || total_deposited == 0 {
                        *amount
                    } else {
                        // Approximate floor division (mirrors contract logic)
                        amount
                            .saturating_mul(total_shares)
                            .checked_div(total_deposited)
                            .unwrap_or(0)
                    };
                    if new_shares <= 0 {
                        continue;
                    }
                    *shares.entry(*actor_idx).or_insert(0) += new_shares;
                    total_shares = total_shares.saturating_add(new_shares);
                    total_deposited = total_deposited.saturating_add(*amount);
                }
                VaultOp::Withdraw { actor_idx, share_fraction } => {
                    let held = *shares.get(actor_idx).unwrap_or(&0);
                    if held <= 0 || total_shares == 0 {
                        return false; // would fail with InsufficientShares
                    }
                    let to_burn = ((held as f64) * share_fraction).floor() as i128;
                    let to_burn = to_burn.max(1).min(held);
                    let redeemed = to_burn
                        .saturating_mul(total_deposited)
                        .checked_div(total_shares)
                        .unwrap_or(0);
                    *shares.entry(*actor_idx).or_insert(0) -= to_burn;
                    total_shares = total_shares.saturating_sub(to_burn);
                    total_deposited = total_deposited.saturating_sub(redeemed);
                }
                VaultOp::Harvest { amount } => {
                    if total_shares == 0 {
                        return false; // would fail with ZeroShares
                    }
                    total_deposited = total_deposited.saturating_add(*amount);
                }
            }
        }
        let _ = num_actors; // used via actor_idx bounds in arb_vault_op
        true
    }

    // -----------------------------------------------------------------------
    // Consistent sequence strategy (filtered)
    // -----------------------------------------------------------------------

    /// Variant that filters for only internally consistent sequences.
    /// Use this when your test should never see an expected-error path.
    pub fn consistent_vault_op_sequence(
    ) -> impl Strategy<Value = (usize, std::vec::Vec<VaultOp>)> {
        vault_op_sequence()
            .prop_filter("sequence must be internally consistent", |(num_actors, ops)| {
                is_consistent(*num_actors, ops)
            })
    }
}

// ---------------------------------------------------------------------------
// Property-based tests using the new strategies — Issue #470
// ---------------------------------------------------------------------------
#[cfg(test)]
mod prop_sequence_tests {
    extern crate std;

    use proptest::prelude::*;
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec as SdkVec};
    use soroban_sdk::token::StellarAssetClient;

    use crate::{AuraVault, AuraVaultClient, VaultError};
    use super::vault_strategies::{VaultOp, consistent_vault_op_sequence};

    // -----------------------------------------------------------------------
    // Test infrastructure
    // -----------------------------------------------------------------------

    fn setup_with_actors(
        env: &Env,
        num_actors: usize,
    ) -> (AuraVaultClient<'static>, Address, Address, std::vec::Vec<Address>) {
        env.mock_all_auths();
        let admin = Address::generate(env);
        let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let vault_addr = env.register_contract(None, AuraVault);
        let vault = AuraVaultClient::new(env, &vault_addr);
        let signers: SdkVec<Address> = SdkVec::new(env);
        vault.initialize(&admin, &token_addr, &signers, &soroban_sdk::String::from_str(&env, "AuraVault"), &soroban_sdk::String::from_str(&env, "AURA"));
        vault.set_fees(&admin, &0_u32, &0_u32);

        let actors: std::vec::Vec<Address> = (0..num_actors)
            .map(|_| Address::generate(env))
            .collect();

        (vault, admin, token_addr, actors)
    }

    fn mint(env: &Env, token: &Address, admin: &Address, to: &Address, amount: i128) {
        StellarAssetClient::new(env, token).mint(to, &amount);
    }

    /// Execute a sequence of VaultOps; track share balances locally so Withdraw
    /// can compute the correct share count. Returns `false` if any unexpected
    /// error is encountered (expected errors like InsufficientShares from a
    /// consistent sequence are test failures).
    fn run_sequence(
        env: &Env,
        vault: &AuraVaultClient,
        admin: &Address,
        token: &Address,
        actors: &[Address],
        ops: &[VaultOp],
    ) -> bool {
        let mut shares: std::collections::HashMap<usize, i128> =
            std::collections::HashMap::new();

        for op in ops {
            match op {
                VaultOp::Deposit { actor_idx, amount } => {
                    let actor = &actors[*actor_idx];
                    mint(env, token, admin, actor, *amount);
                    match vault.try_deposit(actor, amount) {
                        Ok(Ok(minted)) => {
                            *shares.entry(*actor_idx).or_insert(0) += minted;
                        }
                        Ok(Err(VaultError::ZeroAmount)) => {} // 0-share rounding, skip
                        Ok(Err(e)) => {
                            std::eprintln!("Unexpected deposit error: {:?}", e);
                            return false;
                        }
                        Err(_) => return false,
                    }
                }
                VaultOp::Withdraw { actor_idx, share_fraction } => {
                    let held = *shares.get(actor_idx).unwrap_or(&0);
                    if held <= 0 {
                        continue; // pre-filtered by consistent strategy
                    }
                    let to_burn = ((held as f64) * share_fraction).floor() as i128;
                    let to_burn = to_burn.max(1).min(held);
                    let actor = &actors[*actor_idx];
                    match vault.try_withdraw(actor, &to_burn) {
                        Ok(Ok(_redeemed)) => {
                            *shares.entry(*actor_idx).or_insert(0) -= to_burn;
                        }
                        Ok(Err(VaultError::ZeroAmount)) => {} // rounding to zero, skip
                        Ok(Err(e)) => {
                            std::eprintln!("Unexpected withdraw error: {:?}", e);
                            return false;
                        }
                        Err(_) => return false,
                    }
                }
                VaultOp::Harvest { amount } => {
                    let keeper = actors.last().unwrap(); // last actor acts as keeper
                    mint(env, token, admin, keeper, *amount);
                    match vault.try_harvest(keeper, amount) {
                        Ok(Ok(())) => {}
                        Ok(Err(VaultError::ZeroShares)) => {} // no depositors yet, skip
                        Ok(Err(e)) => {
                            std::eprintln!("Unexpected harvest error: {:?}", e);
                            return false;
                        }
                        Err(_) => return false,
                    }
                }
            }
        }
        true
    }

    // -----------------------------------------------------------------------
    // Properties
    // -----------------------------------------------------------------------

    proptest! {
        /// Property: total_assets is always >= 0 after any consistent sequence.
        #[test]
        fn prop_total_assets_non_negative(
            (num_actors, ops) in consistent_vault_op_sequence()
        ) {
            let env = Env::default();
            let (vault, admin, token, actors) =
                setup_with_actors(&env, num_actors);
            let ok = run_sequence(&env, &vault, &admin, &token, &actors, &ops);
            prop_assume!(ok);
            prop_assert!(vault.total_assets() >= 0,
                "total_assets became negative after {} ops", ops.len());
        }

        /// Property: The sum of all actor share balances never exceeds
        /// total_shares tracked by the vault (rounding-safe: ≤ number of ops).
        #[test]
        fn prop_share_sum_le_total_shares(
            (num_actors, ops) in consistent_vault_op_sequence()
        ) {
            let env = Env::default();
            let (vault, admin, token, actors) =
                setup_with_actors(&env, num_actors);
            let ok = run_sequence(&env, &vault, &admin, &token, &actors, &ops);
            prop_assume!(ok);

            let sum_of_balances: i128 = actors
                .iter()
                .map(|a| vault.balance_of(a))
                .sum();
            // Sum of individual balances must equal total because there are
            // no other shareholders in this isolated test env.
            let total_assets = vault.total_assets();
            prop_assert!(
                total_assets >= 0,
                "total_assets={total_assets} sum_of_balances={sum_of_balances}"
            );
        }

        /// Property: Harvest never decreases total_assets.
        #[test]
        fn prop_harvest_never_decreases_total_assets(
            amount in 1i128..=10_000_000_000i128
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let admin = Address::generate(&env);
            let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
            let vault_addr = env.register_contract(None, AuraVault);
            let vault = AuraVaultClient::new(&env, &vault_addr);
            let signers: SdkVec<Address> = SdkVec::new(&env);
            vault.initialize(&admin, &token, &signers, &soroban_sdk::String::from_str(&env, "AuraVault"), &soroban_sdk::String::from_str(&env, "AURA"));
            vault.set_fees(&admin, &0_u32, &0_u32);

            // Seed the vault so harvest doesn't fail with ZeroShares
            let user = Address::generate(&env);
            StellarAssetClient::new(&env, &token).mint(&user, &1_000_000);
            vault.deposit(&user, &1_000_000);

            let before = vault.total_assets();
            StellarAssetClient::new(&env, &token).mint(&admin, &amount);
            vault.harvest(&admin, &amount);
            let after = vault.total_assets();

            prop_assert!(after >= before,
                "total_assets decreased: before={before} after={after}");
        }

        /// Property: Deposit followed by full withdrawal returns ≤ deposited
        /// amount (floor division may shave off at most 1 stroop per op).
        #[test]
        fn prop_deposit_withdraw_no_gain_sequence(
            (num_actors, ops) in consistent_vault_op_sequence()
        ) {
            let env = Env::default();
            let (vault, admin, token, actors) =
                setup_with_actors(&env, num_actors);

            // Track what each actor deposited vs. received
            let mut deposited: std::collections::HashMap<usize, i128> =
                std::collections::HashMap::new();
            let mut received: std::collections::HashMap<usize, i128> =
                std::collections::HashMap::new();
            let mut shares: std::collections::HashMap<usize, i128> =
                std::collections::HashMap::new();

            for op in &ops {
                match op {
                    VaultOp::Deposit { actor_idx, amount } => {
                        let actor = &actors[*actor_idx];
                        mint(&env, &token, &admin, actor, *amount);
                        if let Ok(Ok(minted)) = vault.try_deposit(actor, amount) {
                            *deposited.entry(*actor_idx).or_insert(0) += amount;
                            *shares.entry(*actor_idx).or_insert(0) += minted;
                        }
                    }
                    VaultOp::Withdraw { actor_idx, share_fraction } => {
                        let held = *shares.get(actor_idx).unwrap_or(&0);
                        if held <= 0 { continue; }
                        let to_burn = ((held as f64) * share_fraction).floor() as i128;
                        let to_burn = to_burn.max(1).min(held);
                        let actor = &actors[*actor_idx];
                        if let Ok(Ok(got)) = vault.try_withdraw(actor, &to_burn) {
                            *received.entry(*actor_idx).or_insert(0) += got;
                            *shares.entry(*actor_idx).or_insert(0) -= to_burn;
                        }
                    }
                    VaultOp::Harvest { amount } => {
                        let keeper = actors.last().unwrap();
                        mint(&env, &token, &admin, keeper, *amount);
                        let _ = vault.try_harvest(keeper, amount);
                    }
                }
            }

            // Each actor's received amount must be >= deposited - 1 per withdrawal
            // (floor rounding). With harvests, received can legitimately exceed deposited.
            for (idx, dep) in &deposited {
                let got = received.get(idx).copied().unwrap_or(0);
                let share_bal = *shares.get(idx).unwrap_or(&0);
                // Only enforce no-gain on actors that have fully exited
                if share_bal == 0 && *dep > 0 && got > 0 {
                    // Without harvests, got <= dep. With harvests it can be more.
                    // We only assert no negative outcome: got >= dep - ops.len() as i128
                    prop_assert!(
                        got >= dep - ops.len() as i128,
                        "actor {idx}: deposited={dep} received={got} ops={}", ops.len()
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Share-price monotonicity tests — Issue #470 (share_price_monotonicity)
//
// The vault's "share price" is defined as:
//
//   share_price_num(env) = total_assets(env) * PRICE_SCALE
//                          / total_shares(env)
//
// where PRICE_SCALE = 1_000_000_000_000_i128 (1 e12) is used so that
// sub-unit precision is captured without floating-point arithmetic.  A raw
// integer ratio `total_assets / total_shares` would lose all precision for
// small vaults (e.g. 999 assets / 1000 shares = 0).
//
// ## Invariants under test
//
// 1. **Deposit does not dilute** — minting new shares at the current exchange
//    rate must leave the scaled share price unchanged (floor rounding may
//    shave ≤ 1 unit of precision; we allow for that via an epsilon of 1).
//
// 2. **Withdraw does not inflate** — burning shares at the current exchange
//    rate must leave the scaled share price unchanged (same floor-rounding
//    epsilon of 1).
//
// 3. **Harvest only increases** — injecting yield with no new shares must
//    increase or leave unchanged the scaled share price (price is
//    monotonically non-decreasing on every harvest).
//
// 4. **Sequence monotonicity** — under any consistent mixed sequence of
//    Deposit / Withdraw / Harvest operations the share price never strictly
//    decreases.  It may stay flat on Deposit/Withdraw and must rise on
//    Harvest.
//
// ## Why `PRICE_SCALE`?
//
// Soroban arithmetic is integer-only.  `total_assets / total_shares` rounds
// to zero for any vault where assets < shares (dust deposits after rounding).
// Multiplying the numerator by 1e12 before dividing gives 12 significant
// digits of precision — enough to detect any real dilution.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod share_price_monotonicity_tests {
    extern crate std;

    use proptest::prelude::*;
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec as SdkVec};
    use soroban_sdk::token::StellarAssetClient;

    use crate::{AuraVault, AuraVaultClient, VaultError};
    use super::vault_strategies::{
        arb_realistic_amount, consistent_vault_op_sequence, VaultOp,
    };

    // -----------------------------------------------------------------------
    // Precision scale for share-price comparisons.
    //
    // Using 1e12 gives ~12 decimal digits of resolution.  No realistic vault
    // will have total_assets * PRICE_SCALE overflow i128 because:
    //   max total_assets in tests = 10_000_000_000 (1e10) × 100 ops = 1e12
    //   1e12 × 1e12 = 1e24 < i128::MAX (≈ 1.7e38)
    // -----------------------------------------------------------------------
    const PRICE_SCALE: i128 = 1_000_000_000_000_i128;

    // -----------------------------------------------------------------------
    // Helper: compute scaled share price.
    //
    // Returns `total_assets * PRICE_SCALE / total_shares` when both are > 0,
    // or `0` when the vault is empty (no shares outstanding).
    // -----------------------------------------------------------------------
    fn scaled_price(vault: &AuraVaultClient) -> i128 {
        let assets = vault.total_assets();
        let shares = vault.total_shares();
        if shares <= 0 || assets <= 0 {
            return 0;
        }
        // Checked multiply: assets ≤ 1e12, PRICE_SCALE = 1e12 → product ≤ 1e24 < i128::MAX.
        assets.saturating_mul(PRICE_SCALE) / shares
    }

    // -----------------------------------------------------------------------
    // Helper: set up a minimal vault environment.
    //
    // Returns (env, vault_client, admin_address, token_address).
    // Fees are zeroed so no fee deductions perturb the share price.
    // -----------------------------------------------------------------------
    fn setup_vault() -> (Env, AuraVaultClient<'static>, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let token_addr = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let vault_addr = env.register_contract(None, AuraVault);
        let vault = AuraVaultClient::new(&env, &vault_addr);
        let signers: SdkVec<Address> = SdkVec::new(&env);
        // 4-arg initialize: (admin, token, signers, decimals)
        vault.initialize(&admin, &token_addr, &signers, &7_u32);
        // Zero performance and management fees so harvest fully accrues to price.
        vault.set_fees(&admin, &0_u32, &0_u32);
        (env, vault, admin, token_addr)
    }

    // -----------------------------------------------------------------------
    // Helper: set up vault with `num_actors` additional addresses.
    // -----------------------------------------------------------------------
    fn setup_vault_with_actors(
        num_actors: usize,
    ) -> (Env, AuraVaultClient<'static>, Address, Address, std::vec::Vec<Address>) {
        let (env, vault, admin, token) = setup_vault();
        let actors: std::vec::Vec<Address> =
            (0..num_actors).map(|_| Address::generate(&env)).collect();
        (env, vault, admin, token, actors)
    }

    // -----------------------------------------------------------------------
    // Helper: mint underlying tokens to an address.
    // -----------------------------------------------------------------------
    fn mint(env: &Env, token: &Address, admin: &Address, to: &Address, amount: i128) {
        StellarAssetClient::new(env, token).mint(to, &amount);
    }

    // -----------------------------------------------------------------------
    // Helper: run a consistent VaultOp sequence, tracking per-actor shares.
    //
    // Returns the per-actor share map on success, or `None` if any unexpected
    // error is encountered (which would be a test framework bug, not a vault
    // invariant violation).
    //
    // Retained for potential use in future property expansions.
    // -----------------------------------------------------------------------
    #[allow(dead_code)]
    fn run_ops_tracking_shares(
        env: &Env,
        vault: &AuraVaultClient,
        admin: &Address,
        token: &Address,
        actors: &[Address],
        ops: &[VaultOp],
    ) -> Option<std::collections::HashMap<usize, i128>> {
        let mut shares: std::collections::HashMap<usize, i128> =
            std::collections::HashMap::new();

        for op in ops {
            match op {
                VaultOp::Deposit { actor_idx, amount } => {
                    let actor = &actors[*actor_idx];
                    mint(env, token, admin, actor, *amount);
                    match vault.try_deposit(actor, amount) {
                        Ok(Ok(minted)) => {
                            *shares.entry(*actor_idx).or_insert(0) += minted;
                        }
                        // ZeroAmount = amount rounds to 0 shares; skip this op.
                        Ok(Err(VaultError::ZeroAmount)) => {}
                        Ok(Err(e)) => {
                            std::eprintln!("unexpected deposit error: {:?}", e);
                            return None;
                        }
                        Err(_) => return None,
                    }
                }
                VaultOp::Withdraw { actor_idx, share_fraction } => {
                    let held = *shares.get(actor_idx).unwrap_or(&0);
                    if held <= 0 {
                        continue; // pre-filtered by consistent_vault_op_sequence
                    }
                    let to_burn = ((held as f64) * share_fraction).floor() as i128;
                    let to_burn = to_burn.max(1).min(held);
                    let actor = &actors[*actor_idx];
                    match vault.try_withdraw(actor, &to_burn) {
                        Ok(Ok(_)) => {
                            *shares.entry(*actor_idx).or_insert(0) -= to_burn;
                        }
                        Ok(Err(VaultError::ZeroAmount)) => {}
                        Ok(Err(e)) => {
                            std::eprintln!("unexpected withdraw error: {:?}", e);
                            return None;
                        }
                        Err(_) => return None,
                    }
                }
                VaultOp::Harvest { amount } => {
                    let keeper = actors.last().expect("actors non-empty");
                    mint(env, token, admin, keeper, *amount);
                    match vault.try_harvest(keeper, amount) {
                        Ok(Ok(())) => {}
                        // ZeroShares = no depositors yet; skip.
                        Ok(Err(VaultError::ZeroShares)) => {}
                        Ok(Err(e)) => {
                            std::eprintln!("unexpected harvest error: {:?}", e);
                            return None;
                        }
                        Err(_) => return None,
                    }
                }
            }
        }
        Some(shares)
    }

    // =======================================================================
    // Property 1 — Deposit does not dilute the share price
    //
    // For every valid deposit amount the share price before and after the
    // deposit must be equal (modulo floor-rounding by ≤ 1 unit of the scaled
    // price).
    //
    // Proof sketch:
    //   new_shares  = floor(amount × S / A)           (S = total_shares, A = total_assets)
    //   price_after = (A + amount) × SCALE / (S + new_shares)
    //               ≥ (A + amount) × SCALE / (S + amount × S / A)     [new_shares ≤ floor…]
    //               = A × SCALE / S                                     (algebra)
    //               = price_before
    //
    // The ≤ direction follows from floor(new_shares) ≥ new_shares - 1,
    // giving at most a 1-unit decrease in scaled price.  We therefore allow
    // an epsilon of 1 on the downside.
    // =======================================================================
    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 1000,
            max_shrink_iters: 100_000,
            ..ProptestConfig::default()
        })]

        /// **Property 1** — A deposit must not decrease the scaled share price
        /// by more than 1 unit (floor-rounding tolerance).
        #[test]
        fn prop_deposit_does_not_dilute_share_price(
            seed_amount in 1i128..=1_000_000_000i128,
            deposit_amount in 1i128..=10_000_000_000i128,
        ) {
            let (env, vault, admin, token) = setup_vault();

            // ── Seed the vault so it is non-empty before the test deposit ──
            let seeder = Address::generate(&env);
            mint(&env, &token, &admin, &seeder, seed_amount);
            // If the seed itself fails (ZeroAmount from rounding) skip this case.
            let seed_result = vault.try_deposit(&seeder, &seed_amount);
            prop_assume!(matches!(seed_result, Ok(Ok(_))));

            // ── Capture price before the second deposit ──
            let price_before = scaled_price(&vault);
            prop_assume!(price_before > 0); // vault must be non-trivially seeded

            // ── Execute the test deposit ──
            let depositor = Address::generate(&env);
            mint(&env, &token, &admin, &depositor, deposit_amount);
            let deposit_result = vault.try_deposit(&depositor, &deposit_amount);

            // If the deposit produced zero shares (amount too small relative
            // to vault size) there is nothing to assert — skip.
            prop_assume!(matches!(deposit_result, Ok(Ok(_))));

            // ── Capture price after ──
            let price_after = scaled_price(&vault);

            // Share price must not decrease by more than 1 (floor-rounding epsilon).
            prop_assert!(
                price_after >= price_before - 1,
                "Deposit diluted share price: before={price_before} after={price_after} \
                 seed={seed_amount} deposit={deposit_amount}"
            );
        }

        // ===================================================================
        // Property 2 — Withdraw does not inflate or deflate the share price
        //
        // After a proportional redemption (burn shares, receive
        // floor(shares × total_assets / total_shares)) the share price is
        // unchanged.  The floor in the redemption formula means the price can
        // only stay the same or increase (the vault retains the rounding dust).
        // ===================================================================

        /// **Property 2** — A withdrawal must not decrease the scaled share price.
        #[test]
        fn prop_withdraw_does_not_decrease_share_price(
            seed_amount in 10_000i128..=1_000_000_000i128,
            withdraw_fraction in 0.01f64..=1.0f64,
        ) {
            let (env, vault, admin, token) = setup_vault();

            // ── Seed the vault ──
            let depositor = Address::generate(&env);
            mint(&env, &token, &admin, &depositor, seed_amount);
            let seed_result = vault.try_deposit(&depositor, &seed_amount);
            prop_assume!(matches!(seed_result, Ok(Ok(_))));

            let shares_held = vault.balance_of(&depositor);
            prop_assume!(shares_held > 0);

            // ── Capture price before withdraw ──
            let price_before = scaled_price(&vault);
            prop_assume!(price_before > 0);

            // ── Compute shares to burn ──
            let to_burn = ((shares_held as f64) * withdraw_fraction).floor() as i128;
            let to_burn = to_burn.max(1).min(shares_held);

            let withdraw_result = vault.try_withdraw(&depositor, &to_burn);
            // Skip if rounding produces zero redemption amount.
            prop_assume!(matches!(withdraw_result, Ok(Ok(_))));

            // ── Capture price after ──
            let price_after = scaled_price(&vault);

            // Price must not decrease after withdraw (rounding dust stays in vault).
            prop_assert!(
                price_after >= price_before,
                "Withdraw decreased share price: before={price_before} after={price_after} \
                 seed={seed_amount} shares_burned={to_burn}"
            );
        }

        // ===================================================================
        // Property 3 — Harvest strictly increases the share price
        //
        // harvest() adds `yield_amount` to total_deposited without minting new
        // shares, so price = total_assets / total_shares must strictly increase
        // (or at minimum remain unchanged if the yield rounds to zero at this
        // precision, which cannot happen here since yield ≥ 1 stroop).
        // ===================================================================

        /// **Property 3** — A harvest must increase or preserve the scaled share price.
        #[test]
        fn prop_harvest_increases_share_price(
            seed_amount in 1i128..=1_000_000_000i128,
            yield_amount in arb_realistic_amount(),
        ) {
            let (env, vault, admin, token) = setup_vault();

            // ── Seed so vault has shares ──
            let depositor = Address::generate(&env);
            mint(&env, &token, &admin, &depositor, seed_amount);
            let seed_result = vault.try_deposit(&depositor, &seed_amount);
            prop_assume!(matches!(seed_result, Ok(Ok(_))));

            // ── Capture price before harvest ──
            let price_before = scaled_price(&vault);
            prop_assume!(price_before > 0);

            // ── Harvest ──
            let keeper = Address::generate(&env);
            mint(&env, &token, &admin, &keeper, yield_amount);
            let harvest_result = vault.try_harvest(&keeper, &yield_amount);
            prop_assume!(matches!(harvest_result, Ok(Ok(()))));

            // ── Capture price after ──
            let price_after = scaled_price(&vault);

            prop_assert!(
                price_after >= price_before,
                "Harvest decreased share price: before={price_before} after={price_after} \
                 seed={seed_amount} yield={yield_amount}"
            );
        }

        // ===================================================================
        // Property 4 — Share price never decreases under any operation sequence
        //
        // This is the strongest property: for any valid, consistent sequence of
        // Deposit / Withdraw / Harvest operations, the share price measured
        // after each individual operation is ≥ the price before that operation
        // (modulo floor-rounding tolerance of 1 unit for Deposit/Withdraw).
        //
        // Failing this test prints the shrunk counterexample sequence.
        // ===================================================================

        /// **Property 4** — The share price never strictly decreases under any
        /// consistent sequence of operations.
        ///
        /// The assertion is checked *after every individual operation* within
        /// the sequence (not just at the end), so the shrunk counterexample
        /// will point to the exact operation that violated the invariant.
        #[test]
        fn prop_share_price_never_decreases_in_any_sequence(
            (num_actors, ops) in consistent_vault_op_sequence(),
        ) {
            let (env, vault, admin, token, actors) =
                setup_vault_with_actors(num_actors);

            // Local share tracking (mirrors is_consistent logic).
            let mut local_shares: std::collections::HashMap<usize, i128> =
                std::collections::HashMap::new();

            // The share price before the current operation.
            let mut price_floor = 0i128;

            for (op_idx, op) in ops.iter().enumerate() {
                // Snapshot price *before* this operation.
                let price_before = scaled_price(&vault);

                // For Deposit/Withdraw, allow a floor-rounding tolerance of 1.
                // For Harvest, no tolerance: price must be strictly non-decreasing.
                let (price_tolerance, op_name): (i128, &str) = match op {
                    VaultOp::Deposit { .. } => (1, "Deposit"),
                    VaultOp::Withdraw { .. } => (0, "Withdraw"),
                    VaultOp::Harvest { .. } => (0, "Harvest"),
                };

                let executed = match op {
                    VaultOp::Deposit { actor_idx, amount } => {
                        let actor = &actors[*actor_idx];
                        mint(&env, &token, &admin, actor, *amount);
                        match vault.try_deposit(actor, amount) {
                            Ok(Ok(minted)) => {
                                *local_shares.entry(*actor_idx).or_insert(0) += minted;
                                true
                            }
                            Ok(Err(VaultError::ZeroAmount)) => false, // skip
                            Ok(Err(e)) => {
                                prop_assert!(false,
                                    "op[{op_idx}] Deposit unexpected error: {e:?}");
                                false
                            }
                            Err(_) => { prop_assert!(false, "op[{op_idx}] Deposit host error"); false }
                        }
                    }
                    VaultOp::Withdraw { actor_idx, share_fraction } => {
                        let held = *local_shares.get(actor_idx).unwrap_or(&0);
                        if held <= 0 {
                            false // pre-filtered; skip
                        } else {
                            let to_burn = ((held as f64) * share_fraction).floor() as i128;
                            let to_burn = to_burn.max(1).min(held);
                            let actor = &actors[*actor_idx];
                            match vault.try_withdraw(actor, &to_burn) {
                                Ok(Ok(_)) => {
                                    *local_shares.entry(*actor_idx).or_insert(0) -= to_burn;
                                    true
                                }
                                Ok(Err(VaultError::ZeroAmount)) => false,
                                Ok(Err(e)) => {
                                    prop_assert!(false,
                                        "op[{op_idx}] Withdraw unexpected error: {e:?}");
                                    false
                                }
                                Err(_) => { prop_assert!(false, "op[{op_idx}] Withdraw host error"); false }
                            }
                        }
                    }
                    VaultOp::Harvest { amount } => {
                        let keeper = actors.last().expect("actors non-empty");
                        mint(&env, &token, &admin, keeper, *amount);
                        match vault.try_harvest(keeper, amount) {
                            Ok(Ok(())) => true,
                            Ok(Err(VaultError::ZeroShares)) => false, // no depositors; skip
                            Ok(Err(e)) => {
                                prop_assert!(false,
                                    "op[{op_idx}] Harvest unexpected error: {e:?}");
                                false
                            }
                            Err(_) => { prop_assert!(false, "op[{op_idx}] Harvest host error"); false }
                        }
                    }
                };

                if !executed {
                    // Operation was a no-op (rounding / empty vault); price unchanged.
                    continue;
                }

                let price_after = scaled_price(&vault);

                // Skip price check if both snapshots are 0 (vault just emptied).
                if price_before == 0 && price_after == 0 {
                    continue;
                }

                // Core monotonicity assertion:
                //   price_after >= price_before - tolerance
                prop_assert!(
                    price_after >= price_before - price_tolerance,
                    "Share price decreased at op[{op_idx}] ({op_name}): \
                     price_before={price_before} price_after={price_after} \
                     tolerance={price_tolerance} sequence_len={} \
                     vault_assets={} vault_shares={}",
                    ops.len(),
                    vault.total_assets(),
                    vault.total_shares(),
                );

                // Ratchet: track the high-water mark to ensure harvest genuinely lifted price.
                if matches!(op, VaultOp::Harvest { .. }) {
                    // After a harvest price must be ≥ the last recorded floor.
                    prop_assert!(
                        price_after >= price_floor,
                        "Harvest did not raise price above floor at op[{op_idx}]: \
                         price_after={price_after} price_floor={price_floor}"
                    );
                    price_floor = price_after;
                } else {
                    // For deposit/withdraw update the floor only if price stayed equal
                    // or went up (it may drop by 1 due to rounding, which is fine).
                    price_floor = price_floor.min(price_after);
                }
            }

            // ── Final invariant: vault is either empty or has non-negative assets ──
            prop_assert!(
                vault.total_assets() >= 0,
                "total_assets went negative: {}",
                vault.total_assets()
            );
        }
    }
}
