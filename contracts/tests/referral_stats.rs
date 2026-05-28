//! Tests for the per-referrer stats tracker (#69).
//!
//! `ReferralStorage::record_referred_trade` is admin-gated and idempotent on
//! the unique-trader counter; `Reader::get_referrer_stats` is a passthrough.

#![cfg(test)]

use contracts::{
    reader::{Reader, ReaderClient},
    referral_storage::{ReferralStorage, ReferralStorageClient},
    role_store::{RoleStore, RoleStoreClient},
    types::ReferrerStats,
};
use soroban_sdk::{testutils::Address as _, Address, Env};

struct Fx<'a> {
    rs: ReferralStorageClient<'a>,
    rs_id: Address,
    reader: ReaderClient<'a>,
    admin: Address,
}

fn setup<'a>(env: &'a Env) -> Fx<'a> {
    env.mock_all_auths();
    let admin = Address::generate(env);

    let role_id = env.register(RoleStore, ());
    RoleStoreClient::new(env, &role_id).initialize(&admin);

    let rs_id = env.register(ReferralStorage, ());
    let rs = ReferralStorageClient::new(env, &rs_id);
    rs.initialize(&role_id);

    // Reader needs ds + lh wired even though we only use the referral view.
    use contracts::{
        data_store::{DataStore, DataStoreClient},
        liquidity_handler::{LiquidityHandler, LiquidityHandlerClient},
    };
    let ds_id = env.register(DataStore, ());
    DataStoreClient::new(env, &ds_id).initialize(&admin);
    let lh_id = env.register(LiquidityHandler, ());
    LiquidityHandlerClient::new(env, &lh_id).initialize(&role_id, &ds_id);
    let reader_id = env.register(Reader, ());
    let reader = ReaderClient::new(env, &reader_id);
    reader.initialize(&ds_id, &lh_id);

    Fx { rs, rs_id, reader, admin }
}

#[test]
fn test_get_referrer_stats_defaults_to_zero() {
    let env = Env::default();
    let fx = setup(&env);
    let referrer = Address::generate(&env);
    let s = fx.rs.get_referrer_stats(&referrer);
    assert_eq!(s, ReferrerStats::default());
}

#[test]
fn test_record_referred_trade_accumulates_volume_and_rebates() {
    let env = Env::default();
    let fx = setup(&env);
    let referrer = Address::generate(&env);
    let trader = Address::generate(&env);

    fx.rs
        .record_referred_trade(&fx.admin, &referrer, &trader, &1_000u128, &10u128);
    fx.rs
        .record_referred_trade(&fx.admin, &referrer, &trader, &500u128, &5u128);

    let s = fx.rs.get_referrer_stats(&referrer);
    assert_eq!(s.total_referred_volume_usd, 1_500u128);
    assert_eq!(s.total_rebates_earned, 15u128);
    // Same trader twice — count stays at 1.
    assert_eq!(s.total_traders_referred, 1u32);
}

#[test]
fn test_record_referred_trade_counts_distinct_traders() {
    let env = Env::default();
    let fx = setup(&env);
    let referrer = Address::generate(&env);
    let t1 = Address::generate(&env);
    let t2 = Address::generate(&env);
    let t3 = Address::generate(&env);

    fx.rs.record_referred_trade(&fx.admin, &referrer, &t1, &100u128, &1u128);
    fx.rs.record_referred_trade(&fx.admin, &referrer, &t2, &200u128, &2u128);
    fx.rs.record_referred_trade(&fx.admin, &referrer, &t1, &50u128, &1u128); // repeat
    fx.rs.record_referred_trade(&fx.admin, &referrer, &t3, &300u128, &3u128);

    let s = fx.rs.get_referrer_stats(&referrer);
    assert_eq!(s.total_referred_volume_usd, 650u128);
    assert_eq!(s.total_rebates_earned, 7u128);
    assert_eq!(s.total_traders_referred, 3u32);
}

#[test]
fn test_record_referred_trade_is_per_referrer() {
    let env = Env::default();
    let fx = setup(&env);
    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let trader = Address::generate(&env);

    fx.rs.record_referred_trade(&fx.admin, &r1, &trader, &100u128, &1u128);
    fx.rs.record_referred_trade(&fx.admin, &r2, &trader, &200u128, &2u128);

    let s1 = fx.rs.get_referrer_stats(&r1);
    let s2 = fx.rs.get_referrer_stats(&r2);
    assert_eq!(s1.total_referred_volume_usd, 100u128);
    assert_eq!(s2.total_referred_volume_usd, 200u128);
    assert_eq!(s1.total_traders_referred, 1u32);
    assert_eq!(s2.total_traders_referred, 1u32);
}

#[test]
fn test_reader_get_referrer_stats_passthrough() {
    let env = Env::default();
    let fx = setup(&env);
    let referrer = Address::generate(&env);
    let t1 = Address::generate(&env);
    let t2 = Address::generate(&env);

    fx.rs.record_referred_trade(&fx.admin, &referrer, &t1, &10u128, &1u128);
    fx.rs.record_referred_trade(&fx.admin, &referrer, &t2, &90u128, &9u128);

    let s = fx.reader.get_referrer_stats(&fx.rs_id, &referrer);
    assert_eq!(s.total_referred_volume_usd, 100u128);
    assert_eq!(s.total_rebates_earned, 10u128);
    assert_eq!(s.total_traders_referred, 2u32);
}
