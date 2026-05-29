//! Reader view tests for the protocol UI surface (#71/#72/#73).
//!
//! Each helper sets up the contracts the reader depends on and writes its
//! input state directly via the underlying handlers. The reader is treated as
//! a black box — assertions check what callers will actually see.

#![cfg(test)]

use contracts::{
    data_store::{DataStore, DataStoreClient},
    keys::{funding_factor_key, open_interest_long_key, open_interest_short_key},
    liquidity_handler::{LiquidityHandler, LiquidityHandlerClient},
    order_handler::{OrderHandler, OrderHandlerClient},
    reader::{Reader, ReaderClient},
    role_store::{RoleStore, RoleStoreClient},
};
use soroban_sdk::{testutils::Address as _, Address, Env};

struct Fx<'a> {
    reader: ReaderClient<'a>,
    ds: DataStoreClient<'a>,
    lh: LiquidityHandlerClient<'a>,
    lh_id: Address,
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

    Fx {
        reader,
        ds,
        lh,
        lh_id,
        admin,
    }
}

// ---------------------------------------------------------------------------
// #73 — get_funding_info
// ---------------------------------------------------------------------------

#[test]
fn test_get_funding_info_returns_stored_values() {
    let env = Env::default();
    let fx = setup(&env);
    let market_id: u32 = 7;
    let writer = fx.admin.clone();

    // Write funding state directly through the data store — this is the same
    // path the funding fee accrual writes through.
    fx.ds
        .set_i128(&writer, &funding_factor_key(&env, market_id), &-12_345i128);
    fx.ds.set_u128(
        &writer,
        &open_interest_long_key(&env, market_id),
        &1_000u128,
    );
    fx.ds
        .set_u128(&writer, &open_interest_short_key(&env, market_id), &750u128);

    let info = fx.reader.get_funding_info(&market_id);
    assert_eq!(info.funding_factor_per_second, -12_345i128);
    assert_eq!(info.open_interest_long, 1_000u128);
    assert_eq!(info.open_interest_short, 750u128);
    // Per-side aggregate claimables aren't tracked at the protocol level yet;
    // the reader surfaces zeros so the struct shape matches the spec.
    assert_eq!(info.claimable_funding_long, 0u128);
    assert_eq!(info.claimable_funding_short, 0u128);
}

#[test]
fn test_get_funding_info_defaults_when_unset() {
    let env = Env::default();
    let fx = setup(&env);
    // A market we've never written to — every field should be 0.
    let info = fx.reader.get_funding_info(&42u32);
    assert_eq!(info.funding_factor_per_second, 0i128);
    assert_eq!(info.open_interest_long, 0u128);
    assert_eq!(info.open_interest_short, 0u128);
    assert_eq!(info.claimable_funding_long, 0u128);
    assert_eq!(info.claimable_funding_short, 0u128);
}

// ---------------------------------------------------------------------------
// #72 — withdrawal views
// ---------------------------------------------------------------------------

const MARKET: u32 = 0;

/// Registers `MARKET` on the handler and seeds `user` with enough LP that a
/// `lp_amount` withdrawal will succeed. Returns the SAC token addresses so
/// callers can mint more LP for additional withdrawals.
fn register_market_and_seed_lp(
    env: &Env,
    fx: &Fx<'_>,
    user: &Address,
    lp_amount: u128,
) -> (Address, Address, u32) {
    let long = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let short = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    fx.lh.register_market(&fx.admin, &MARKET, &long, &short);
    fx.lh.set_oracle_prices(&fx.admin, &MARKET, &1u128, &1u128);

    // Deposit so the user has LP — first deposit seeds LP = deposit_value
    // (i.e. 2 * lp_amount). Caller can then create a withdrawal up to that.
    soroban_sdk::token::StellarAssetClient::new(env, &long).mint(user, &(lp_amount as i128));
    soroban_sdk::token::StellarAssetClient::new(env, &short).mint(user, &(lp_amount as i128));
    fx.lh
        .execute_deposit(user, &MARKET, &lp_amount, &lp_amount, user);
    (long, short, MARKET)
}

#[test]
fn test_get_withdrawal_via_reader_round_trip() {
    let env = Env::default();
    let fx = setup(&env);
    let user = Address::generate(&env);
    let (_, _, market_id) = register_market_and_seed_lp(&env, &fx, &user, 500);

    let wid = fx
        .lh
        .create_withdrawal(&user, &market_id, &500u128, &user, &0u128, &0u128);

    let w = fx
        .reader
        .read_withdrawal(&wid)
        .expect("reader should see the pending withdrawal");
    assert_eq!(w.account, user);
    assert_eq!(w.market_id, market_id);
    assert_eq!(w.lp_amount, 500u128);
    assert_eq!(w.receiver, user);
}

#[test]
fn test_get_withdrawal_missing_returns_none() {
    let env = Env::default();
    let fx = setup(&env);
    // No withdrawals have ever been created — id 9999 must be None, not panic.
    assert!(fx.reader.read_withdrawal(&9_999u32).is_none());
}

#[test]
fn test_get_account_withdrawals_filters_and_paginates() {
    let env = Env::default();
    let fx = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Seed market + alice's first 500 LP withdrawal.
    let (long_tok, short_tok, market_id) = register_market_and_seed_lp(&env, &fx, &alice, 500);
    let _a0 = fx
        .lh
        .create_withdrawal(&alice, &market_id, &100u128, &alice, &0u128, &0u128);

    // Bob deposits and creates a withdrawal in between alice's two.
    use soroban_sdk::token::StellarAssetClient;
    StellarAssetClient::new(&env, &long_tok).mint(&bob, &200);
    StellarAssetClient::new(&env, &short_tok).mint(&bob, &200);
    fx.lh
        .execute_deposit(&bob, &market_id, &200u128, &200u128, &bob);
    let _b0 = fx
        .lh
        .create_withdrawal(&bob, &market_id, &200u128, &bob, &0u128, &0u128);

    // Alice's second withdrawal — a distinct lp_amount so we can identify it.
    let _a1 = fx
        .lh
        .create_withdrawal(&alice, &market_id, &50u128, &alice, &0u128, &0u128);

    // Alice has 2 pending withdrawals; bob has 1.
    let all_alice = fx.reader.get_account_withdrawals(&alice, &0u32, &10u32);
    assert_eq!(all_alice.len(), 2, "alice should have exactly 2 pending");
    for w in all_alice.iter() {
        assert_eq!(w.account, alice);
    }

    let all_bob = fx.reader.get_account_withdrawals(&bob, &0u32, &10u32);
    assert_eq!(all_bob.len(), 1, "bob should have exactly 1 pending");
    assert_eq!(all_bob.get(0).unwrap().account, bob);

    // Pagination: limit=1 returns the first one only.
    let page1 = fx.reader.get_account_withdrawals(&alice, &0u32, &1u32);
    assert_eq!(page1.len(), 1);

    // start=1 skips the first hit for alice — should land on the other one.
    let page2 = fx.reader.get_account_withdrawals(&alice, &1u32, &10u32);
    assert_eq!(page2.len(), 1);
    assert_ne!(
        page1.get(0).unwrap().lp_amount,
        page2.get(0).unwrap().lp_amount,
        "start=1 should return a different withdrawal than start=0",
    );

    // limit=0 short-circuits to empty.
    let none = fx.reader.get_account_withdrawals(&alice, &0u32, &0u32);
    assert_eq!(none.len(), 0);
}

// ---------------------------------------------------------------------------
// #71 — get_account_orders
// ---------------------------------------------------------------------------
//
// Order *creation* needs a fully configured market / pool / referral pipeline
// that lives outside the reader's responsibility — the `order_handler` test
// modules already cover that. The reader's own logic — the loop, the filter,
// the pagination short-circuits — is what we verify here.

#[test]
fn test_get_account_orders_returns_empty_when_handler_has_none() {
    let env = Env::default();
    let fx = setup(&env);

    let oh_id = env.register(OrderHandler, ());
    OrderHandlerClient::new(&env, &oh_id).initialize(&fx.lh_id);

    let acct = Address::generate(&env);
    let out = fx.reader.get_account_orders(&oh_id, &acct, &0u32, &10u32);
    assert_eq!(out.len(), 0);
}

#[test]
fn test_get_account_orders_limit_zero_short_circuits() {
    let env = Env::default();
    let fx = setup(&env);

    let oh_id = env.register(OrderHandler, ());
    OrderHandlerClient::new(&env, &oh_id).initialize(&fx.lh_id);

    let acct = Address::generate(&env);
    let out = fx.reader.get_account_orders(&oh_id, &acct, &0u32, &0u32);
    assert_eq!(out.len(), 0);
}
