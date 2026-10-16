use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
pub enum DataKey {
    Admin,
    UnderlyingToken,
    TotalShares,
    TotalDeposited,
    Balance(Address),
    Version,
    LayoutVersion,
    /// Emergency pause flag — when true, deposit/withdraw/harvest are blocked.
    Paused,
    /// Fee system keys (instance storage)
    Treasury,
    PerfFeeBps,
    MgmtFeeBps,
    TotalFeeCollected,
    LastMgmtFeeTime,
    /// TWAP oracle keys.
    /// Head = next write index in the circular buffer (instance storage).
    TwapHead,
    /// Len  = number of valid entries, 0 ..= BUFFER_SIZE (instance storage).
    TwapLen,
    /// Individual snapshot slot, indexed 0 .. BUFFER_SIZE-1 (persistent storage).
    TwapSlot(u32),
}

// ---------------------------------------------------------------------------
// TWAP types and constants
// ---------------------------------------------------------------------------

/// A single (timestamp, price) observation stored in the circular buffer.
#[contracttype]
#[derive(Clone)]
pub struct PriceSnapshot {
    pub ts: u64,
    pub price: i128,
}

/// Maximum number of snapshots kept (hourly for 24h).
pub const TWAP_BUFFER_SIZE: u32 = 24;

/// Fixed-point scale for share price (1e9).
pub const PRICE_SCALE: i128 = 1_000_000_000;

pub const DAY_IN_LEDGERS: u32 = 17_280;
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = DAY_IN_LEDGERS * 7;
pub const INSTANCE_BUMP_AMOUNT: u32 = DAY_IN_LEDGERS * 30;
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = DAY_IN_LEDGERS * 7;
pub const PERSISTENT_BUMP_AMOUNT: u32 = DAY_IN_LEDGERS * 30;

// ---------------------------------------------------------------------------
// Instance-storage helpers
// ---------------------------------------------------------------------------

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::Admin)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn get_token(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::UnderlyingToken)
}

pub fn set_token(env: &Env, token: &Address) {
    env.storage().instance().set(&DataKey::UnderlyingToken, token);
}

pub fn get_total_shares(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::TotalShares).unwrap_or(0)
}

pub fn set_total_shares(env: &Env, val: i128) {
    env.storage().instance().set(&DataKey::TotalShares, &val);
}

pub fn get_total_deposited(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::TotalDeposited).unwrap_or(0)
}

pub fn set_total_deposited(env: &Env, val: i128) {
    env.storage().instance().set(&DataKey::TotalDeposited, &val);
}

// ---------------------------------------------------------------------------
// Persistent-storage helpers
// ---------------------------------------------------------------------------

pub fn get_balance(env: &Env, addr: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(addr.clone()))
        .unwrap_or(0)
}

pub fn set_balance(env: &Env, addr: &Address, val: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::Balance(addr.clone()), &val);
}

// ---------------------------------------------------------------------------
// Fee storage helpers (instance storage)
// ---------------------------------------------------------------------------

pub fn get_treasury(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::Treasury)
}

pub fn set_treasury(env: &Env, treasury: &Address) {
    env.storage().instance().set(&DataKey::Treasury, treasury);
}

pub fn get_perf_fee_bps(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::PerfFeeBps).unwrap_or(0)
}

pub fn set_perf_fee_bps(env: &Env, bps: u32) {
    env.storage().instance().set(&DataKey::PerfFeeBps, &bps);
}

pub fn get_mgmt_fee_bps(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::MgmtFeeBps).unwrap_or(0)
}

pub fn set_mgmt_fee_bps(env: &Env, bps: u32) {
    env.storage().instance().set(&DataKey::MgmtFeeBps, &bps);
}

pub fn get_total_fee_collected(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::TotalFeeCollected).unwrap_or(0)
}

pub fn set_total_fee_collected(env: &Env, val: i128) {
    env.storage().instance().set(&DataKey::TotalFeeCollected, &val);
}

pub fn get_last_mgmt_fee_time(env: &Env) -> u64 {
    env.storage().instance().get(&DataKey::LastMgmtFeeTime).unwrap_or(0)
}

pub fn set_last_mgmt_fee_time(env: &Env, time: u64) {
    env.storage().instance().set(&DataKey::LastMgmtFeeTime, &time);
}

// ---------------------------------------------------------------------------
// TTL bump helpers
// ---------------------------------------------------------------------------

pub fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

pub fn bump_persistent(env: &Env, addr: &Address) {
    env.storage()
        .persistent()
        .extend_ttl(
            &DataKey::Balance(addr.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
}

// ---------------------------------------------------------------------------
// Version helpers (instance storage — same TTL as the rest of state)
// ---------------------------------------------------------------------------

/// Current storage layout constant. Bump this in source whenever a new
/// DataKey variant changes an existing key's meaning.
pub const CURRENT_LAYOUT_VERSION: u32 = 1;

pub fn get_version(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::Version).unwrap_or(0)
}

pub fn set_version(env: &Env, v: u32) {
    env.storage().instance().set(&DataKey::Version, &v);
}

pub fn get_layout_version(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::LayoutVersion)
        .unwrap_or(0)
}

pub fn set_layout_version(env: &Env, v: u32) {
    env.storage().instance().set(&DataKey::LayoutVersion, &v);
}

// ---------------------------------------------------------------------------
// Pause helpers (instance storage)
// ---------------------------------------------------------------------------

pub fn is_paused(env: &Env) -> bool {
    env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
}

pub fn set_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&DataKey::Paused, &paused);
}

// ---------------------------------------------------------------------------
// TWAP oracle helpers
// ---------------------------------------------------------------------------

/// Insert a new price snapshot into the circular buffer.
///
/// Buffer layout: `TWAP_BUFFER_SIZE` slots (persistent storage), addressed by
/// `DataKey::TwapSlot(i)`.  `TwapHead` holds the index where the *next* entry
/// will be written; `TwapLen` tracks how many entries are valid (saturating at
/// `TWAP_BUFFER_SIZE` once the buffer is full).
pub fn twap_push(env: &Env, snapshot: &PriceSnapshot) {
    let head: u32 = env.storage().instance().get(&DataKey::TwapHead).unwrap_or(0);
    let len: u32 = env.storage().instance().get(&DataKey::TwapLen).unwrap_or(0);

    // Write at current head position
    env.storage()
        .persistent()
        .set(&DataKey::TwapSlot(head), snapshot);

    // Bump TTL for the slot we just wrote
    env.storage()
        .persistent()
        .extend_ttl(
            &DataKey::TwapSlot(head),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

    // Advance head with wrap-around
    let new_head = (head + 1) % TWAP_BUFFER_SIZE;
    env.storage().instance().set(&DataKey::TwapHead, &new_head);

    // Increment length up to buffer size
    let new_len = if len < TWAP_BUFFER_SIZE { len + 1 } else { len };
    env.storage().instance().set(&DataKey::TwapLen, &new_len);
}

/// Read the snapshot at logical index `i` (0 = oldest, len-1 = newest).
///
/// The oldest entry is at `(head - len) mod BUFFER_SIZE`.  We compute the
/// physical slot from the logical index and read it from persistent storage.
pub fn twap_get(env: &Env, i: u32) -> PriceSnapshot {
    let head: u32 = env.storage().instance().get(&DataKey::TwapHead).unwrap_or(0);
    let len: u32 = env.storage().instance().get(&DataKey::TwapLen).unwrap_or(0);

    // oldest physical slot = (head - len + BUFFER_SIZE) mod BUFFER_SIZE
    let oldest = (head + TWAP_BUFFER_SIZE - len) % TWAP_BUFFER_SIZE;
    let slot_idx = (oldest + i) % TWAP_BUFFER_SIZE;

    env.storage()
        .persistent()
        .get(&DataKey::TwapSlot(slot_idx))
        .unwrap_or(PriceSnapshot { ts: 0, price: 0 })
}

/// Current number of valid snapshots in the buffer (0 ..= BUFFER_SIZE).
pub fn twap_len(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::TwapLen).unwrap_or(0)
}

/// Bump TTL on all TWAP slots that contain valid data.
pub fn bump_twap_slots(env: &Env) {
    let len = twap_len(env);
    let head: u32 = env.storage().instance().get(&DataKey::TwapHead).unwrap_or(0);
    let oldest = (head + TWAP_BUFFER_SIZE - len) % TWAP_BUFFER_SIZE;
    for i in 0..len {
        let slot_idx = (oldest + i) % TWAP_BUFFER_SIZE;
        env.storage()
            .persistent()
            .extend_ttl(
                &DataKey::TwapSlot(slot_idx),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
    }
}