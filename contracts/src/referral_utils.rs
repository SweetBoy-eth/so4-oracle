//! Referral rebate and discount application during order execution.

use soroban_sdk::{Address, BytesN, Env};

use crate::{
    data_store::DataStoreClient,
    keys::claimable_referral_amount_key,
    referral_storage::ReferralStorageClient,
    types::TierConfig,
};

pub const REFERRAL_FACTOR_DENOMINATOR: u128 = 10_000;

/// Compute the position fee before referral adjustments.
pub fn compute_position_fee(size_delta_usd: u128, position_fee_factor: u128) -> u128 {
    if size_delta_usd == 0 || position_fee_factor == 0 {
        return 0;
    }
    size_delta_usd
        .saturating_mul(position_fee_factor)
        / crate::pricing_utils::FACTOR_DENOMINATOR
}

/// Split a gross position fee into trader discount and referrer rebate per tier.
pub fn split_referral_fee(position_fee: u128, tier: &TierConfig) -> (u128, u128) {
    if position_fee == 0 {
        return (0, 0);
    }
    let discount = position_fee
        .saturating_mul(tier.discount_bps as u128)
        / REFERRAL_FACTOR_DENOMINATOR;
    let rebate = position_fee
        .saturating_mul(tier.rebate_bps as u128)
        / REFERRAL_FACTOR_DENOMINATOR;
    (discount, rebate)
}

/// Apply referral rebates for a position trade when a referral code is set.
///
/// Returns the net position fee charged to the trader after discount.
pub fn apply_referral_rebates(
    env: &Env,
    ds: &DataStoreClient,
    rs: &ReferralStorageClient,
    writer: &Address,
    referral_code: &BytesN<32>,
    position_fee: u128,
    fee_token: &Address,
) -> u128 {
    if position_fee == 0 {
        return 0;
    }

    if referral_code.to_array() == [0u8; 32] {
        return position_fee;
    }

    let code_owner = match rs.get_code_owner(&referral_code) {
        Some(owner) => owner,
        None => return position_fee,
    };

    let tier = rs.get_tier(&code_owner);
    let (discount, rebate) = split_referral_fee(position_fee, &tier);

    let claimable_key = claimable_referral_amount_key(env, &code_owner, fee_token);
    let current = ds.get_u128(&claimable_key).unwrap_or(0);
    ds.set_u128(writer, &claimable_key, &(current + rebate));

    position_fee.saturating_sub(discount)
}
