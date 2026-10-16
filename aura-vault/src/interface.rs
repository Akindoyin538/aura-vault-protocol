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

    /// Time-weighted average price of a vault share over `window_seconds`.
    /// Returns the spot price if fewer than 2 observations exist.
    fn get_twap(env: Env, window_seconds: u64) -> i128;
}
