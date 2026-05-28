//! Tests for the Reader's open interest views (#74).
//!
//! The reader is a thin read-through; we seed OI directly via the data store
//! and verify the values surface intact, including a couple of common pricing
//! edge cases (zero oracle price, partial-side OI).

#![cfg(test)]

use contracts::{
    data_store::{DataStore, DataStoreClient},
    keys::{open_interest_long_key, open_interest_short_key},
    liquidity_handler::{LiquidityHandler, LiquidityHandlerClient},
    reader::{Reader, ReaderClient},
    role_store::{RoleStore, RoleStoreClient},
};
use soroban_sdk::{testutils::Address as _, Address, Env};

struct Fx<'a> {
    reader: ReaderClient<'a>,
    ds: DataStoreClient<'a>,
    lh: LiquidityHandlerClient<'a>,
    admin: Address,
}

fn setup<'a>(env: &'a Env) -> Fx<'a> {
    env.mock_all_auths();
    let admin = Address::generate(env);

    let rs_id = env.register(RoleStore, ());
    RoleStoreClient::new(env, &rs_id).initialize(&admin);

    let ds_id = env.register(DataStore, ());
    let ds = DataStoreClient::new(env, &ds_id);
    ds.initialize(&admin);

    let lh_id = env.register(LiquidityHandler, ());
    let lh = LiquidityHandlerClient::new(env, &lh_id);
    lh.initialize(&rs_id, &ds_id);

    let reader_id = env.register(Reader, ());
    let reader = ReaderClient::new(env, &reader_id);
    reader.initialize(&ds_id, &lh_id);

    Fx { reader, ds, lh, admin }
}

#[test]
fn test_get_open_interest_defaults_to_zero_for_unknown_market() {
    let env = Env::default();
    let fx = setup(&env);
    let (long, short) = fx.reader.get_open_interest(&999u32);
    assert_eq!(long, 0u128);
    assert_eq!(short, 0u128);
}

#[test]
fn test_get_open_interest_reads_both_sides() {
    let env = Env::default();
    let fx = setup(&env);
    let m = 3u32;

    // Simulate "3 long + 2 short positions" — what matters here is the
    // aggregate USD OI that the position pipeline accumulates.
    fx.ds.set_u128(&fx.admin, &open_interest_long_key(&env, m), &3_000u128);
    fx.ds.set_u128(&fx.admin, &open_interest_short_key(&env, m), &2_000u128);

    let (long, short) = fx.reader.get_open_interest(&m);
    assert_eq!(long, 3_000u128);
    assert_eq!(short, 2_000u128);
}

#[test]
fn test_get_open_interest_in_tokens_divides_by_oracle_price() {
    let env = Env::default();
    let fx = setup(&env);
    let m = 5u32;

    fx.ds.set_u128(&fx.admin, &open_interest_long_key(&env, m), &10_000u128);
    fx.ds.set_u128(&fx.admin, &open_interest_short_key(&env, m), &6_000u128);
    fx.lh.set_oracle_prices(&fx.admin, &m, &100u128, &50u128);

    let (long_in_tokens, short_in_tokens) = fx.reader.get_open_interest_in_tokens(&m);
    assert_eq!(long_in_tokens, 100u128);  // 10_000 / 100
    assert_eq!(short_in_tokens, 120u128); // 6_000 / 50
}

#[test]
fn test_get_open_interest_in_tokens_zero_price_does_not_panic() {
    let env = Env::default();
    let fx = setup(&env);
    let m = 11u32;

    fx.ds.set_u128(&fx.admin, &open_interest_long_key(&env, m), &10u128);
    // No oracle prices set — get_open_interest_in_tokens should return (0, 0)
    // instead of panicking from a div-by-zero.
    let (long_in_tokens, short_in_tokens) = fx.reader.get_open_interest_in_tokens(&m);
    assert_eq!(long_in_tokens, 0u128);
    assert_eq!(short_in_tokens, 0u128);
}

#[test]
fn test_get_open_interest_in_tokens_zero_side() {
    let env = Env::default();
    let fx = setup(&env);
    let m = 7u32;

    fx.ds.set_u128(&fx.admin, &open_interest_long_key(&env, m), &10_000u128);
    // Short OI not set — defaults to 0.
    fx.lh.set_oracle_prices(&fx.admin, &m, &100u128, &50u128);

    let (long_in_tokens, short_in_tokens) = fx.reader.get_open_interest_in_tokens(&m);
    assert_eq!(long_in_tokens, 100u128);
    assert_eq!(short_in_tokens, 0u128);
}
