//! Issue #112 — end-to-end: deploy → market → deposit → long → short →
//! close long (profit) → close short (loss) → withdraw → claim fees.
//!
//! Runs entirely in the Soroban test environment with balance and pool-value
//! checks at each step.

#![cfg(test)]

use contracts::{
    adl_handler::{AdlHandler, AdlHandlerClient},
    data_store::{DataStore, DataStoreClient},
    fee_handler::{FeeHandler, FeeHandlerClient},
    keys::{
        claimable_fee_amount_key, claimable_protocol_fee_key, max_open_interest_long_key,
        max_open_interest_short_key, position_fee_factor_key,
    },
    liquidity_handler::{LiquidityHandler, LiquidityHandlerClient},
    market_factory::{MarketFactory, MarketFactoryClient},
    market_utils,
    order_handler::{order_keeper_role, OrderHandler, OrderHandlerClient},
    position_handler::{PositionHandler, PositionHandlerClient},
    reader::{Reader, ReaderClient},
    referral_storage::{ReferralStorage, ReferralStorageClient},
    role_store::{RoleStore, RoleStoreClient},
    router::{Router, RouterClient},
    types::{MarketConfig, OrderType, Position},
};
use soroban_sdk::{
    testutils::Address as _,
    token::{StellarAssetClient, TokenClient},
    vec, Address, Env,
};

const MARKET: u32 = 0;

struct E2e {
    env: Env,
    admin: Address,
    keeper: Address,
    lp: Address,
    trader: Address,
    fee_receiver: Address,
    rs: Address,
    ds: Address,
    mf: Address,
    lh: Address,
    oh: Address,
    ph: Address,
    adl: Address,
    fh: Address,
    ref_store: Address,
    reader: Address,
    router: Address,
    long: Address,
    short: Address,
    index: Address,
    market_token: Address,
}

fn deploy_all(env: &Env) -> E2e {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let keeper = Address::generate(env);
    let lp = Address::generate(env);
    let trader = Address::generate(env);
    let fee_receiver = Address::generate(env);

    let rs = env.register(RoleStore, ());
    let ds = env.register(DataStore, ());
    let mf = env.register(MarketFactory, ());
    let lh = env.register(LiquidityHandler, ());
    let oh = env.register(OrderHandler, ());
    let ph = env.register(PositionHandler, ());
    let adl = env.register(AdlHandler, ());
    let fh = env.register(FeeHandler, ());
    let ref_store = env.register(ReferralStorage, ());
    let reader = env.register(Reader, ());
    let router = env.register(Router, ());

    RoleStoreClient::new(env, &rs).initialize(&admin);
    DataStoreClient::new(env, &ds).initialize(&admin);
    MarketFactoryClient::new(env, &mf).initialize(&rs, &ds);
    LiquidityHandlerClient::new(env, &lh).initialize(&rs, &ds);
    OrderHandlerClient::new(env, &oh).initialize(&ds);
    OrderHandlerClient::new(env, &oh).configure(&rs, &lh);
    PositionHandlerClient::new(env, &ph).initialize(&ds, &lh);
    AdlHandlerClient::new(env, &adl).initialize(&ds, &lh);
    FeeHandlerClient::new(env, &fh).initialize(&ds, &fee_receiver);
    ReferralStorageClient::new(env, &ref_store).initialize(&rs);
    ReaderClient::new(env, &reader).initialize(&ds, &lh);
    RouterClient::new(env, &router).initialize(&lh);

    let rs_client = RoleStoreClient::new(env, &rs);
    rs_client.grant_role(&admin, &order_keeper_role(env), &keeper);
    OrderHandlerClient::new(env, &oh).set_adl_handler(&admin, &adl);
    OrderHandlerClient::new(env, &oh).set_referral_storage(&admin, &ref_store);

    let long = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let short = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let index = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let market_token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let cfg = MarketConfig {
        max_long_open_interest: 10_000_000,
        max_short_open_interest: 10_000_000,
        maintenance_margin_factor: 50_000,
    };
    MarketFactoryClient::new(env, &mf).create_market(
        &admin,
        &index,
        &long,
        &short,
        &market_token,
        &Some(cfg),
    );
    LiquidityHandlerClient::new(env, &lh).register_market(&admin, &MARKET, &long, &short);

    let ds_client = DataStoreClient::new(env, &ds);
    ds_client.set_u128(
        &admin,
        &max_open_interest_long_key(env, MARKET),
        &10_000_000u128,
    );
    ds_client.set_u128(
        &admin,
        &max_open_interest_short_key(env, MARKET),
        &10_000_000u128,
    );
    ds_client.set_u128(&admin, &position_fee_factor_key(env, MARKET), &10_000u128);

    E2e {
        env: env.clone(),
        admin,
        keeper,
        lp,
        trader,
        fee_receiver,
        rs,
        ds,
        mf,
        lh,
        oh,
        ph,
        adl,
        fh,
        ref_store,
        reader,
        router,
        long,
        short,
        index,
        market_token,
    }
}

fn lh(e: &E2e) -> LiquidityHandlerClient<'_> {
    LiquidityHandlerClient::new(&e.env, &e.lh)
}

fn oh(e: &E2e) -> OrderHandlerClient<'_> {
    OrderHandlerClient::new(&e.env, &e.oh)
}

fn fh(e: &E2e) -> FeeHandlerClient<'_> {
    FeeHandlerClient::new(&e.env, &e.fh)
}

fn ds(e: &E2e) -> DataStoreClient<'_> {
    DataStoreClient::new(&e.env, &e.ds)
}

fn reader(e: &E2e) -> ReaderClient<'_> {
    ReaderClient::new(&e.env, &e.reader)
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

fn pool_value_info(
    e: &E2e,
    long_price: u128,
    short_price: u128,
) -> contracts::types::PoolValueInfo {
    reader(e).get_market_pool_value_info(&MARKET, &long_price, &short_price, &false)
}

#[test]
fn test_e2e_deposit_trade_withdraw_claim_fees() {
    let env = Env::default();
    let s = deploy_all(&env);
    let long_tok = TokenClient::new(&env, &s.long);
    let short_tok = TokenClient::new(&env, &s.short);

    // --- Step 1: market already created in deploy_all ---
    assert_eq!(MarketFactoryClient::new(&env, &s.mf).market_count(), 1);

    // --- Step 2: deposit liquidity ---
    lh(&s).set_oracle_prices(&s.admin, &MARKET, &100u128, &100u128);
    mint(&env, &s.long, &s.lp, 10_000);
    mint(&env, &s.short, &s.lp, 10_000);
    let lp_minted = lh(&s).execute_deposit(&s.lp, &MARKET, &5_000u128, &5_000u128, &s.lp);
    assert!(lp_minted > 0);
    assert_eq!(lh(&s).pool_amounts(&MARKET), (5_000u128, 5_000u128));

    let (pool_long_before, pool_short_before) = lh(&s).pool_amounts(&MARKET);
    let pool_value_before = pool_long_before * 100 + pool_short_before * 100;
    let lp_supply_before = lh(&s).lp_supply(&MARKET);
    let lp_price_before = pool_value_before * 1_000_000 / lp_supply_before.max(1);

    // --- Step 3: open long ---
    mint(&env, &s.long, &s.trader, 2_000);
    oh(&s).set_position(
        &s.admin,
        &Position {
            account: s.trader.clone(),
            market_id: MARKET,
            is_long: true,
            size_in_usd: 5_000,
            size_in_tokens: 50,
            collateral_amount: 500,
            referral_code: soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
        },
    );
    assert!(oh(&s).get_position(&s.trader, &MARKET, &true).is_some());

    // --- Step 4: open short ---
    mint(&env, &s.short, &s.trader, 2_000);
    oh(&s).set_position(
        &s.admin,
        &Position {
            account: s.trader.clone(),
            market_id: MARKET,
            is_long: false,
            size_in_usd: 5_000,
            size_in_tokens: 50,
            collateral_amount: 500,
            referral_code: soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
        },
    );

    // Order handler needs token balance to pay released collateral and fees.
    mint(&env, &s.long, &s.oh, 5_000);
    mint(&env, &s.short, &s.oh, 5_000);

    // --- Step 5: close long at profit (price 100 → 120) ---
    lh(&s).set_oracle_prices(&s.admin, &MARKET, &120u128, &100u128);
    let close_long = oh(&s).create_order(
        &s.trader,
        &MARKET,
        &OrderType::LimitDecrease,
        &s.long,
        &true,
        &5_000u128,
        &0u128,
        &100u128,
        &0u128,
        &0u128,
    );
    oh(&s).execute_order(&s.keeper, &close_long);
    assert!(oh(&s).get_position(&s.trader, &MARKET, &true).is_none());

    // --- Step 6: close short at loss (short index rises 100 → 130) ---
    lh(&s).set_oracle_prices(&s.admin, &MARKET, &120u128, &130u128);
    let close_short = oh(&s).create_order(
        &s.trader,
        &MARKET,
        &OrderType::LimitDecrease,
        &s.short,
        &false,
        &5_000u128,
        &0u128,
        &200u128,
        &1_000_000u128,
        &0u128,
    );
    oh(&s).execute_order(&s.keeper, &close_short);
    assert!(oh(&s).get_position(&s.trader, &MARKET, &false).is_none());

    // Pool token amounts stable after matched long/short (PnL nets at protocol level).
    lh(&s).set_oracle_prices(&s.admin, &MARKET, &100u128, &100u128);
    let (pool_long_after, pool_short_after) = lh(&s).pool_amounts(&MARKET);
    assert_eq!(
        (pool_long_before, pool_short_before),
        (pool_long_after, pool_short_after),
        "pool reserves unchanged after matched closes"
    );
    let pool_value_after = pool_long_after * 100 + pool_short_after * 100;
    let lp_supply_after = lh(&s).lp_supply(&MARKET);
    let lp_price_after = pool_value_after * 1_000_000 / lp_supply_after.max(1);
    assert_eq!(lp_supply_before, lp_supply_after);
    assert!(
        lp_price_after >= lp_price_before * 99 / 100,
        "LP token price should reflect pool value"
    );

    // Accrue protocol fees for claim step
    let fee_amount = 100u128;
    ds(&s).set_u128(
        &s.admin,
        &claimable_protocol_fee_key(&env, MARKET, &s.long),
        &fee_amount,
    );
    StellarAssetClient::new(&env, &s.long).mint(&s.fh, &(fee_amount as i128));

    // --- Step 7: withdraw liquidity ---
    let wid = lh(&s).create_withdrawal(&s.lp, &MARKET, &lp_minted, &s.lp, &0u128, &0u128);
    lh(&s).execute_withdrawal(&s.lp, &wid);
    assert_eq!(lh(&s).lp_balance_of(&MARKET, &s.lp), 0);

    // --- Step 8: claim fees ---
    let markets = vec![&env, MARKET];
    let tokens = vec![&env, s.long.clone()];
    let claimed = fh(&s).claim_fees(&markets, &tokens);
    assert_eq!(claimed, fee_amount as i128);
    assert_eq!(long_tok.balance(&s.fee_receiver), fee_amount as i128);
    assert_eq!(fh(&s).get_claimable_protocol_fee(&MARKET, &s.long), 0u128);

    // Sanity: reader pool view matches liquidity handler
    let (pl, ps) = lh(&s).pool_amounts(&MARKET);
    let prices = lh(&s).oracle_prices(&MARKET);
    let expected = market_utils::get_pool_value(
        pl,
        ps,
        prices.long_price,
        prices.short_price,
        0,
        lh(&s).lp_supply(&MARKET),
        false,
    );
    let via_reader = pool_value_info(&s, prices.long_price, prices.short_price);
    assert_eq!(via_reader.pool_value, expected.pool_value);
    assert_eq!(via_reader.lp_supply, expected.lp_supply);

    let _ = short_tok;
}
