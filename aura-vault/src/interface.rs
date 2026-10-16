use soroban_sdk::{Address, BytesN, Env};
use crate::errors::VaultError;

/// Public ABI for AuraVault.  Implemented by the contract in lib.rs.
#[allow(dead_code)]
pub trait AuraVaultTrait {
    fn initialize(env: Env, admin: Address, underlying_token: Address) -> Result<(), VaultError>;
    fn deposit(env: Env, caller: Address, amount: i128) -> Result<i128, VaultError>;
    fn withdraw(env: Env, caller: Address, shares: i128) -> Result<i128, VaultError>;
    fn harvest(env: Env, caller: Address, yield_amount: i128) -> Result<(), VaultError>;
    fn pause(env: Env) -> Result<(), VaultError>;
    fn unpause(env: Env) -> Result<(), VaultError>;
    fn is_paused(env: Env) -> bool;
    fn total_assets(env: Env) -> i128;
    fn balance_of(env: Env, address: Address) -> i128;
    /// Transfer admin role to a new address.  Only the current admin may call this.
    fn transfer_admin(env: Env, new_admin: Address) -> Result<(), VaultError>;
    /// UUPS-style upgrade: replace the contract's Wasm with `new_wasm_hash`.
    /// Only the admin may call this.  Emits an `upgrade` event.
    fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), VaultError>;
    /// Returns the current monotonic version number (starts at 1 after initialize).
    fn version(env: Env) -> u32;

    // --- Admin parameter setters (each emits a `parameter_changed` event) ---

    /// Set performance fee (bps) and management fee (bps).
    /// Emits `parameter_changed` for both `perf_fee_bps` and `mgmt_fee_bps`.
    fn set_fees(env: Env, perf_fee_bps: u32, mgmt_fee_bps: u32) -> Result<(), VaultError>;

    /// Set the treasury address that receives collected fees.
    /// Emits `parameter_changed` for `treasury`.
    fn set_treasury(env: Env, treasury: Address) -> Result<(), VaultError>;

    /// Set the minimum yield required before a harvest is accepted.
    /// Emits `parameter_changed` for `harvest_minimum`.
    fn set_harvest_minimum(env: Env, minimum: i128) -> Result<(), VaultError>;

    /// Set the maximum total-value-locked cap (0 = uncapped).
    /// Deposits that would push `total_deposited` above this value are rejected.
    /// Emits `parameter_changed` for `tvl_cap`.
    fn set_tvl_cap(env: Env, cap: i128) -> Result<(), VaultError>;

    /// Read current fee configuration: (perf_fee_bps, mgmt_fee_bps).
    fn get_fees(env: Env) -> (u32, u32);

    /// Read total fees accrued but not yet withdrawn.
    fn total_fees_collected(env: Env) -> i128;

    /// Transfer all accrued fees to the treasury.  Admin-only.
    fn withdraw_fees(env: Env) -> Result<i128, VaultError>;
}
