use soroban_sdk::{Address, BytesN, Env};
use soroban_sdk::xdr::ToXdr;

// ---------------------------------------------------------------------------
// Market key generators
// ---------------------------------------------------------------------------

/// Returns the data-store key that holds the config for `market_id`.
pub fn market_props_key(env: &Env, market_id: u32) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..6].copy_from_slice(b"mprops");
    let id_bytes = market_id.to_be_bytes();
    buf[6..10].copy_from_slice(&id_bytes);
    BytesN::from_array(env, &buf)
}

/// Returns the data-store key for the long-token address of `market_id`.
pub fn market_long_token_key(env: &Env, market_id: u32) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(b"mlt_addr");
    let id_bytes = market_id.to_be_bytes();
    buf[8..12].copy_from_slice(&id_bytes);
    BytesN::from_array(env, &buf)
}

/// Returns the data-store key for the short-token address of `market_id`.
pub fn market_short_token_key(env: &Env, market_id: u32) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(b"mst_addr");
    let id_bytes = market_id.to_be_bytes();
    buf[8..12].copy_from_slice(&id_bytes);
    BytesN::from_array(env, &buf)
}

/// Returns the data-store key for the market-token address of `market_id`.
pub fn market_token_key(env: &Env, market_id: u32) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..7].copy_from_slice(b"mtkaddr");
    let id_bytes = market_id.to_be_bytes();
    buf[7..11].copy_from_slice(&id_bytes);
    BytesN::from_array(env, &buf)
}

/// Returns the data-store key that holds a `u128` flag (1 = paused) for
/// `market_id`.
pub fn market_paused_key(env: &Env, market_id: u32) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(b"mpaused_");
    let id_bytes = market_id.to_be_bytes();
    buf[8..12].copy_from_slice(&id_bytes);
    BytesN::from_array(env, &buf)
}

/// Returns the data-store key holding the total number of markets ever created.
pub fn market_count_key(env: &Env) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..9].copy_from_slice(b"mkt_count");
    BytesN::from_array(env, &buf)
}

// ---------------------------------------------------------------------------
// Liquidity key generators
// ---------------------------------------------------------------------------

fn market_scoped_key(env: &Env, prefix: &[u8; 8], market_id: u32) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(prefix);
    buf[8..12].copy_from_slice(&market_id.to_be_bytes());
    BytesN::from_array(env, &buf)
}

pub fn pool_long_amount_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"plong_am", market_id)
}

pub fn pool_short_amount_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"pshrt_am", market_id)
}

pub fn claimable_fee_long_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"clmfee_l", market_id)
}

pub fn claimable_fee_short_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"clmfee_s", market_id)
}

pub fn withdrawal_fee_factor_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"wfeefact", market_id)
}

pub fn market_maintenance_margin_factor_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"mm_factr", market_id)
}

pub fn position_oi_list_key(env: &Env, market_id: u32, is_long: bool) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..7].copy_from_slice(b"poilist");
    let id_bytes = market_id.to_be_bytes();
    buf[7..11].copy_from_slice(&id_bytes);
    buf[11] = if is_long { 1 } else { 0 };
    BytesN::from_array(env, &buf)
}

pub fn position_list_key(env: &Env) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(b"pos_list");
    BytesN::from_array(env, &buf)
}

// ---------------------------------------------------------------------------
// Position / open-interest key generators
// ---------------------------------------------------------------------------

pub fn open_interest_long_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"oi_long_", market_id)
}

pub fn open_interest_short_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"oi_shrt_", market_id)
}

pub fn max_open_interest_long_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"maxoi_lo", market_id)
}

pub fn max_open_interest_short_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"maxoi_sh", market_id)
}

pub fn account_balance_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"acct_bal", market_id)
}

pub fn claimable_fee_amount_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"clmfee_a", market_id)
}

pub fn market_order_expiry_ledgers_key(env: &Env) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..16].copy_from_slice(b"mkt_ord_exp_legd");
    BytesN::from_array(env, &buf)
}

pub fn limit_order_expiry_ledgers_key(env: &Env) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..16].copy_from_slice(b"lmt_ord_exp_legd");
    BytesN::from_array(env, &buf)
}

// ---------------------------------------------------------------------------
// Config handler key generators
// ---------------------------------------------------------------------------

pub fn max_pool_amount_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"cfgmxpam", market_id)
}

pub fn max_open_interest_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"cfgmxpoi", market_id)
}

pub fn position_fee_factor_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"cfgpffee", market_id)
}

pub fn borrowing_factor_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"cfgbrrwf", market_id)
}

pub fn funding_factor_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"cfgfundf", market_id)
}

pub fn min_collateral_factor_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"cfgmncol", market_id)
}

pub fn max_leverage_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"cfgmxlev", market_id)
}

// ---------------------------------------------------------------------------
// ADL key generators
// ---------------------------------------------------------------------------

pub fn max_pnl_factor_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"maxpnlfc", market_id)
}

// ---------------------------------------------------------------------------
// Pricing key generators
// ---------------------------------------------------------------------------

pub fn price_impact_factor_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"pimpactf", market_id)
}

/// Fixed-point (FACTOR_DENOMINATOR-scaled) exponent applied to the imbalance
/// term of the price-impact curve. `1_000_000` means `^1` (linear).
pub fn price_impact_exponent_factor_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"pimpexpf", market_id)
}

/// Balance of the per-market impact pool that funds favorable price impact.
/// Favorable impact paid to traders can never exceed this balance.
pub fn impact_pool_amount_key(env: &Env, market_id: u32) -> BytesN<32> {
    market_scoped_key(env, b"impactpl", market_id)
}

// ---------------------------------------------------------------------------
// Referral key generators
// ---------------------------------------------------------------------------

pub fn claimable_referral_amount_key(env: &Env, code_owner: &Address, token: &Address) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(b"clmref_a");
    let owner_bytes: [u8; 32] = env.crypto().sha256(&code_owner.to_xdr(env)).to_array();
    let token_bytes: [u8; 32] = env.crypto().sha256(&token.to_xdr(env)).to_array();
    buf[8..16].copy_from_slice(&owner_bytes[..8]);
    buf[16..24].copy_from_slice(&token_bytes[..8]);
    BytesN::from_array(env, &buf)
}

/// Per-receiver / market / token UI fee claimable balance (#70).
pub fn ui_claimable_fee_amount_key(
    env: &Env,
    receiver: &Address,
    market_id: u32,
    token: &Address,
) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(b"uiclm_fe");
    let r: [u8; 32] = env.crypto().sha256(&receiver.to_xdr(env)).to_array();
    let t: [u8; 32] = env.crypto().sha256(&token.to_xdr(env)).to_array();
    buf[8..16].copy_from_slice(&r[..8]);
    buf[16..24].copy_from_slice(&t[..8]);
    buf[24..28].copy_from_slice(&market_id.to_be_bytes());
    BytesN::from_array(env, &buf)
}

/// Per-market / token claimable funding balance for an account (#67).
pub fn claimable_funding_amount_key(
    env: &Env,
    market_id: u32,
    token: &Address,
    account: &Address,
) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(b"clmfundf");
    let t: [u8; 32] = env.crypto().sha256(&token.to_xdr(env)).to_array();
    let a: [u8; 32] = env.crypto().sha256(&account.to_xdr(env)).to_array();
    buf[8..12].copy_from_slice(&market_id.to_be_bytes());
    buf[12..20].copy_from_slice(&t[..8]);
    buf[20..28].copy_from_slice(&a[..8]);
    BytesN::from_array(env, &buf)
}

/// Cumulative referrer statistics key (#69).
pub fn referrer_stats_key(env: &Env, referrer: &Address) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(b"refstats");
    let r: [u8; 32] = env.crypto().sha256(&referrer.to_xdr(env)).to_array();
    buf[8..32].copy_from_slice(&r[..24]);
    BytesN::from_array(env, &buf)
}

// ---------------------------------------------------------------------------
// Fee handler key generators (#66)
// ---------------------------------------------------------------------------

/// Protocol-fee accumulator per market+token. Distinct from
/// `claimable_fee_amount_key` (swap fees, market-only) — this is the slot the
/// new `FeeHandler::claim_fees` entry point reads, scoped by token because
/// post-#66 markets may accrue fees in multiple denominations.
pub fn claimable_protocol_fee_key(env: &Env, market_id: u32, token: &Address) -> BytesN<32> {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(b"clmpr_fe");
    buf[8..12].copy_from_slice(&market_id.to_be_bytes());
    let token_bytes: [u8; 32] = env.crypto().sha256(&token.to_xdr(env)).to_array();
    buf[12..32].copy_from_slice(&token_bytes[..20]);
    BytesN::from_array(env, &buf)
}
