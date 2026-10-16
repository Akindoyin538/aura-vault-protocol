#![no_std]

mod errors;
mod interface;
mod storage;
mod fee;

pub use errors::VaultError;

#[cfg(test)]
mod test;

#[cfg(test)]
mod fuzz;

use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, Symbol};

use storage::{
    bump_instance, bump_persistent, get_admin, get_balance, get_layout_version, get_token,
    get_total_deposited, get_total_shares, get_version, is_paused as storage_is_paused, set_admin,
    set_balance, set_layout_version, set_paused, set_token, set_total_deposited, set_total_shares,
    set_version, CURRENT_LAYOUT_VERSION,
};

#[contract]
pub struct AuraVault;

#[contractimpl]
impl AuraVault {
    // -----------------------------------------------------------------------
    // initialize
    // -----------------------------------------------------------------------
    pub fn initialize(
        env: Env,
        admin: Address,
        underlying_token: Address,
    ) -> Result<(), VaultError> {
        if get_admin(&env).is_some() {
            return Err(VaultError::AlreadyInitialized);
        }
        set_admin(&env, &admin);
        set_token(&env, &underlying_token);
        set_total_shares(&env, 0);
        set_total_deposited(&env, 0);
        set_version(&env, 1);
        set_layout_version(&env, CURRENT_LAYOUT_VERSION);
        bump_instance(&env);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // deposit
    // -----------------------------------------------------------------------
    pub fn deposit(env: Env, caller: Address, amount: i128) -> Result<i128, VaultError> {
        caller.require_auth();

        if amount <= 0 {
            return Err(VaultError::ZeroAmount);
        }
        if get_admin(&env).is_none() {
            return Err(VaultError::NotInitialized);
        }
        if storage_is_paused(&env) {
            return Err(VaultError::VaultPaused);
        }

        let token_addr = get_token(&env).ok_or(VaultError::NotInitialized)?;
        let token = token::Client::new(&env, &token_addr);

        // Flash-loan guard: actual token balance must equal tracked state before deposit.
        let balance_before = token.balance(&env.current_contract_address());
        let total_deposited = get_total_deposited(&env);
        if balance_before != total_deposited {
            env.events().publish(
                (Symbol::new(&env, "suspicious"),),
                (Symbol::new(&env, "balance_mismatch"), balance_before, total_deposited),
            );
            return Err(VaultError::BalanceMismatch);
        }

        let total_shares = get_total_shares(&env);

        // Compute shares to mint (checked arithmetic, overflow returns MathOverflow)
        let new_shares: i128 = if total_shares == 0 || total_deposited == 0 {
            amount
        } else {
            let numerator = amount
                .checked_mul(total_shares)
                .ok_or(VaultError::MathOverflow)?;
            numerator
                .checked_div(total_deposited)
                .ok_or(VaultError::MathOverflow)?
        };

        if new_shares <= 0 {
            return Err(VaultError::ZeroAmount);
        }

        // CEI — Interaction: pull tokens from caller into vault
        token.transfer(&caller, &env.current_contract_address(), &amount);

        // Effects: write state after successful transfer
        let old_balance = get_balance(&env, &caller);
        let new_balance = old_balance + new_shares;
        set_balance(&env, &caller, new_balance);
        let new_total_shares = total_shares
            .checked_add(new_shares)
            .ok_or(VaultError::MathOverflow)?;
        set_total_shares(&env, new_total_shares);
        let new_total_deposited = total_deposited
            .checked_add(amount)
            .ok_or(VaultError::MathOverflow)?;
        set_total_deposited(&env, new_total_deposited);

        env.events().publish(
            (Symbol::new(&env, "deposit"),),
            (caller.clone(), amount, new_shares),
        );

        bump_persistent(&env, &caller);
        bump_instance(&env);

        // FIX-3: Emit event for off-chain indexers and monitoring
        env.events().publish(
            (Symbol::new(&env, "deposit"),),
            (caller, amount, new_shares, new_total_shares, new_total_deposited),
        );

        Ok(new_shares)
    }

    // -----------------------------------------------------------------------
    // withdraw
    // -----------------------------------------------------------------------
    pub fn withdraw(env: Env, caller: Address, shares: i128) -> Result<i128, VaultError> {
        caller.require_auth();

        if shares <= 0 {
            return Err(VaultError::ZeroAmount);
        }
        if get_admin(&env).is_none() {
            return Err(VaultError::NotInitialized);
        }
        if storage_is_paused(&env) {
            return Err(VaultError::VaultPaused);
        }

        let token_addr = get_token(&env).ok_or(VaultError::NotInitialized)?;
        let token = token::Client::new(&env, &token_addr);

        // Flash-loan guard: actual token balance must equal tracked state.
        let balance_before = token.balance(&env.current_contract_address());
        let total_deposited = get_total_deposited(&env);
        if balance_before != total_deposited {
            env.events().publish(
                (Symbol::new(&env, "suspicious"),),
                (Symbol::new(&env, "balance_mismatch"), balance_before, total_deposited),
            );
            return Err(VaultError::BalanceMismatch);
        }

        let user_balance = get_balance(&env, &caller);
        if shares > user_balance {
            return Err(VaultError::InsufficientShares);
        }

        let total_shares = get_total_shares(&env);

        let numerator = shares
            .checked_mul(total_deposited)
            .ok_or(VaultError::MathOverflow)?;
        let redeem_amount = numerator
            .checked_div(total_shares)
            .ok_or(VaultError::MathOverflow)?;

        if redeem_amount <= 0 {
            return Err(VaultError::ZeroAmount);
        }
        if total_deposited < redeem_amount {
            return Err(VaultError::InsufficientUnderlying);
        }

        // CEI — Effects first: burn shares before token transfer
        let new_balance = user_balance - shares;
        set_balance(&env, &caller, new_balance);
        let new_total_shares = total_shares
            .checked_sub(shares)
            .ok_or(VaultError::MathOverflow)?;
        set_total_shares(&env, new_total_shares);
        let new_total_deposited = total_deposited
            .checked_sub(redeem_amount)
            .ok_or(VaultError::MathOverflow)?;
        set_total_deposited(&env, new_total_deposited);

        // Interaction: send tokens to caller after state is settled
        token.transfer(&env.current_contract_address(), &caller, &redeem_amount);

        env.events().publish(
            (Symbol::new(&env, "withdraw"),),
            (caller.clone(), shares, redeem_amount),
        );

        bump_persistent(&env, &caller);
        bump_instance(&env);

        // FIX-3: Emit event for off-chain indexers and monitoring
        env.events().publish(
            (Symbol::new(&env, "withdraw"),),
            (caller, shares, redeem_amount, new_total_shares, new_total_deposited),
        );

        Ok(redeem_amount)
    }

    // -----------------------------------------------------------------------
    // harvest
    // -----------------------------------------------------------------------
    pub fn harvest(env: Env, caller: Address, yield_amount: i128) -> Result<(), VaultError> {
        caller.require_auth();

        if yield_amount <= 0 {
            return Err(VaultError::ZeroAmount);
        }
        if get_admin(&env).is_none() {
            return Err(VaultError::NotInitialized);
        }
        if storage_is_paused(&env) {
            return Err(VaultError::VaultPaused);
        }

        // FIX-2: Only the admin may call harvest.
        // Permissionless harvest allows any token-holder to inflate total_deposited
        // without sending real yield, or to grief by triggering dust rounding at
        // will.  Restricting to admin (or a keeper registry added later) closes
        // the open-access vector without affecting the happy-path workflow.
        let admin = get_admin(&env).ok_or(VaultError::NotInitialized)?;
        if caller != admin {
            return Err(VaultError::HarvestUnauthorized);
        }

        let total_shares = get_total_shares(&env);
        if total_shares == 0 {
            return Err(VaultError::ZeroShares);
        }

        let total_deposited = get_total_deposited(&env);

        let token_addr = get_token(&env).ok_or(VaultError::NotInitialized)?;
        let token = token::Client::new(&env, &token_addr);

        // Flash-loan guard: actual token balance must equal tracked state before harvest.
        let balance_before = token.balance(&env.current_contract_address());
        if balance_before != total_deposited {
            env.events().publish(
                (Symbol::new(&env, "suspicious"),),
                (Symbol::new(&env, "balance_mismatch"), balance_before, total_deposited),
            );
            return Err(VaultError::BalanceMismatch);
        }

        // Calculate performance fee on yield
        let perf_fee_bps = storage::get_perf_fee_bps(&env);
        let perf_fee = fee::calc_perf_fee(yield_amount, perf_fee_bps)?;

        // Yield after performance fee goes into the vault; fee is tracked separately
        let yield_after_fee = yield_amount
            .checked_sub(perf_fee)
            .ok_or(VaultError::MathOverflow)?;

        let new_total = total_deposited
            .checked_add(yield_after_fee)
            .ok_or(VaultError::MathOverflow)?;

        // Interaction: pull yield tokens into vault
        token.transfer(&caller, &env.current_contract_address(), &yield_amount);

        // Effect: increase total deposited with yield after fees, record fees
        set_total_deposited(&env, new_total);

        // Record accrued performance fee
        let total_fee = storage::get_total_fee_collected(&env)
            .checked_add(perf_fee)
            .ok_or(VaultError::MathOverflow)?;
        storage::set_total_fee_collected(&env, total_fee);

        // Record TWAP snapshot — share price after this harvest
        let ts = env.ledger().timestamp();
        let price = share_price(&env, new_total, total_shares);
        storage::twap_push(&env, &storage::PriceSnapshot { ts, price });
        storage::bump_twap_slots(&env);

        env.events().publish(
            (Symbol::new(&env, "harvest"),),
            (caller, yield_amount),
        );

        bump_instance(&env);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // pause / unpause — admin-only emergency controls
    // -----------------------------------------------------------------------
    pub fn pause(env: Env) -> Result<(), VaultError> {
        let admin = get_admin(&env).ok_or(VaultError::NotInitialized)?;
        admin.require_auth();
        set_paused(&env, true);
        env.events().publish((Symbol::new(&env, "paused"),), ());
        bump_instance(&env);
        Ok(())
    }

    pub fn unpause(env: Env) -> Result<(), VaultError> {
        let admin = get_admin(&env).ok_or(VaultError::NotInitialized)?;
        admin.require_auth();
        set_paused(&env, false);
        env.events().publish((Symbol::new(&env, "unpaused"),), ());
        bump_instance(&env);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        storage_is_paused(&env)
    }

    // -----------------------------------------------------------------------
    // total_assets  (read-only — no bumps, no writes)
    // -----------------------------------------------------------------------
    pub fn total_assets(env: Env) -> i128 {
        get_total_deposited(&env)
    }

    // -----------------------------------------------------------------------
    // balance_of  (read-only — no bumps, no writes)
    // -----------------------------------------------------------------------
    pub fn balance_of(env: Env, address: Address) -> i128 {
        get_balance(&env, &address)
    }

    // -----------------------------------------------------------------------
    // transfer_admin — FIX-6: admin is no longer immutable
    // -----------------------------------------------------------------------
    /// Transfers admin role to `new_admin`.  Only the current admin may call
    /// this.  Emits a `admin_transferred` event so monitors can detect a
    /// privilege hand-off immediately.
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), VaultError> {
        let admin = get_admin(&env).ok_or(VaultError::NotInitialized)?;
        admin.require_auth();

        set_admin(&env, &new_admin);
        bump_instance(&env);

        env.events().publish(
            (Symbol::new(&env, "admin_transferred"),),
            (admin, new_admin),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // upgrade — UUPS-style: admin-only Wasm replacement
    // -----------------------------------------------------------------------
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), VaultError> {
        // 1. Must be initialized
        let admin = get_admin(&env).ok_or(VaultError::NotInitialized)?;

        // 2. Admin-only authorization
        admin.require_auth();

        // 3. Layout compatibility guard
        if get_layout_version(&env) != CURRENT_LAYOUT_VERSION {
            return Err(VaultError::StorageLayoutMismatch);
        }

        // 4. Effects: bump version before Wasm swap
        let next_version = get_version(&env)
            .checked_add(1)
            .ok_or(VaultError::MathOverflow)?;
        set_version(&env, next_version);
        bump_instance(&env);

        // 5. Emit upgrade event BEFORE swap (still in current Wasm execution context)
        env.events().publish(
            (Symbol::new(&env, "upgrade"),),
            (admin, next_version, new_wasm_hash.clone()),
        );

        // 6. Interaction: atomic Wasm replacement (point of no return)
        env.deployer().update_current_contract_wasm(new_wasm_hash);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // version  (read-only)
    // -----------------------------------------------------------------------
    pub fn version(env: Env) -> u32 {
        get_version(&env)
    }

    // -----------------------------------------------------------------------
    // TWAP oracle — time-weighted average price of a vault share
    // -----------------------------------------------------------------------

    /// Returns the time-weighted average price of one vault share over the
    /// last `window_seconds` seconds.
    ///
    /// The price is expressed in fixed-point with `PRICE_SCALE` (1e9) decimals:
    /// `price = (total_assets / total_shares) * PRICE_SCALE`.
    ///
    /// If fewer than 2 observations exist the spot price is returned instead.
    /// If the vault has no shares (total_shares == 0) the price is defined as
    /// `PRICE_SCALE` (1:1) to avoid division by zero.
    pub fn get_twap(env: Env, window_seconds: u64) -> i128 {
        let len = storage::twap_len(&env);

        // Fewer than 2 observations → return spot price
        if len < 2 {
            return share_price(&env, get_total_deposited(&env), get_total_shares(&env));
        }

        let now = env.ledger().timestamp();
        let cutoff = now.saturating_sub(window_seconds);

        // Walk the buffer from newest to oldest, collecting observations that
        // fall within the window.  The newest snapshot is at logical index
        // `len - 1`.
        //
        // TWAP = Σ(price_i × duration_i) / Σ(duration_i)
        // where duration_i is the time between observation i and i+1.

        let mut weighted_sum: i128 = 0;
        let mut total_weight: u64 = 0;

        // Start from the newest observation
        let mut prev_ts: u64 = now;

        for i in (0..len).rev() {
            let snap = storage::twap_get(&env, i);

            // If this observation is older than the cutoff, it's the boundary
            // point.  The duration for the segment above it is clipped to
            // (prev_ts - cutoff) rather than (prev_ts - snap.ts).
            if snap.ts <= cutoff {
                let segment = prev_ts.saturating_sub(cutoff);
                if segment > 0 {
                    weighted_sum = weighted_sum
                        .checked_add(
                            snap.price
                                .checked_mul(segment as i128)
                                .unwrap_or(0),
                        )
                        .unwrap_or(0);
                    total_weight = total_weight.saturating_add(segment);
                }
                break;
            }

            // Full segment between this observation and the previous one
            let segment = prev_ts.saturating_sub(snap.ts);
            if segment > 0 {
                weighted_sum = weighted_sum
                    .checked_add(
                        snap.price
                            .checked_mul(segment as i128)
                            .unwrap_or(0),
                    )
                    .unwrap_or(0);
                total_weight = total_weight.saturating_add(segment);
            }

            prev_ts = snap.ts;
        }

        if total_weight == 0 {
            // All observations are newer than the window → return spot price
            return share_price(&env, get_total_deposited(&env), get_total_shares(&env));
        }

        weighted_sum
            .checked_div(total_weight as i128)
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Fee Management Functions
    // -----------------------------------------------------------------------
    
    /// Set performance and management fees (admin only)
    pub fn set_fees(env: Env, perf_fee_bps: u32, mgmt_fee_bps: u32) -> Result<(), VaultError> {
        let admin = storage::get_admin(&env).ok_or(VaultError::NotInitialized)?;
        admin.require_auth();
        
        fee::validate_fees(perf_fee_bps, mgmt_fee_bps)?;
        
        storage::set_perf_fee_bps(&env, perf_fee_bps);
        storage::set_mgmt_fee_bps(&env, mgmt_fee_bps);
        storage::bump_instance(&env);
        
        Ok(())
    }

    /// Set treasury address (admin only)
    pub fn set_treasury(env: Env, treasury: Address) -> Result<(), VaultError> {
        let admin = storage::get_admin(&env).ok_or(VaultError::NotInitialized)?;
        admin.require_auth();
        
        storage::set_treasury(&env, &treasury);
        storage::bump_instance(&env);
        
        Ok(())
    }

    /// Get current fee settings
    pub fn get_fees(env: Env) -> (u32, u32) {
        (storage::get_perf_fee_bps(&env), storage::get_mgmt_fee_bps(&env))
    }

    /// Get total fees collected
    pub fn total_fees_collected(env: Env) -> i128 {
        storage::get_total_fee_collected(&env)
    }

    /// Withdraw fees to treasury (admin only)
    pub fn withdraw_fees(env: Env) -> Result<i128, VaultError> {
        let admin = storage::get_admin(&env).ok_or(VaultError::NotInitialized)?;
        admin.require_auth();
        
        let treasury = storage::get_treasury(&env).ok_or(VaultError::InvalidAddress)?;
        let fees = storage::get_total_fee_collected(&env);
        
        if fees > 0 {
            let token_addr = storage::get_token(&env).ok_or(VaultError::NotInitialized)?;
            token::Client::new(&env, &token_addr).transfer(
                &env.current_contract_address(),
                &treasury,
                &fees,
            );
            
            storage::set_total_fee_collected(&env, 0);
            storage::bump_instance(&env);
        }
        
        Ok(fees)
    }
}

// ---------------------------------------------------------------------------
// Internal helper: compute the current share price in fixed-point
// ---------------------------------------------------------------------------

/// Compute the share price as `(total_assets / total_shares) * PRICE_SCALE`.
/// If `total_shares` is zero, returns `PRICE_SCALE` (1:1) to avoid div-by-zero.
fn share_price(_env: &Env, total_assets: i128, total_shares: i128) -> i128 {
    if total_shares == 0 {
        return storage::PRICE_SCALE;
    }
    total_assets
        .checked_mul(storage::PRICE_SCALE)
        .unwrap_or(0)
        .checked_div(total_shares)
        .unwrap_or(0)
}
