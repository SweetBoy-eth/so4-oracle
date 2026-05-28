//! UI fee claim tests for #70.
//!
//! The fee handler holds the pool of pending UI fees; balances are tracked in
//! the data store under `ui_claimable_fee_amount_key`. Wiring the *accrual*
//! sites (i.e. position-fee paths writing into these slots and transferring
//! the underlying tokens) is the production follow-up referenced in the PR
//! body. These tests verify the claim entry point in isolation: storage is
//! seeded directly, the contract is funded with tokens, then `claim_ui_fees`
//! is exercised.

#![cfg(test)]

use contracts::{
    data_store::{DataStore, DataStoreClient},
    fee_handler::{FeeHandler, FeeHandlerClient},
    keys::ui_claimable_fee_amount_key,
    role_store::{RoleStore, RoleStoreClient},
};
use soroban_sdk::{
    testutils::Address as _,
    token::{StellarAssetClient, TokenClient},
    vec, Address, Env, Vec,
};

struct Fx<'a> {
    fh: FeeHandlerClient<'a>,
    fh_id: Address,
    ds: DataStoreClient<'a>,
    admin: Address,
}

fn setup<'a>(env: &'a Env) -> Fx<'a> {
    env.mock_all_auths();

    let admin = Address::generate(env);

    // RoleStore is required so the data store's writer auth works, even though
    // FeeHandler itself does not consult role_store.
    let rs_id = env.register(RoleStore, ());
    RoleStoreClient::new(env, &rs_id).initialize(&admin);

    let ds_id = env.register(DataStore, ());
    let ds = DataStoreClient::new(env, &ds_id);
    ds.initialize(&admin);

    let fh_id = env.register(FeeHandler, ());
    let fh = FeeHandlerClient::new(env, &fh_id);
    fh.initialize(&ds_id);

    Fx { fh, fh_id, ds, admin }
}

#[test]
fn test_initialize_panics_on_double_init() {
    let env = Env::default();
    let fx = setup(&env);
    let other_ds = env.register(DataStore, ());
    let result = fx.fh.try_initialize(&other_ds);
    assert!(result.is_err(), "second initialize must fail");
}

#[test]
fn test_get_ui_claimable_fee_defaults_zero() {
    let env = Env::default();
    let fx = setup(&env);

    let receiver = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();

    assert_eq!(fx.fh.get_ui_claimable_fee(&receiver, &0u32, &token), 0u128);
}

#[test]
fn test_get_ui_claimable_fee_reflects_storage() {
    let env = Env::default();
    let fx = setup(&env);

    let receiver = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();

    // Seed the per-receiver / market / token balance directly.
    let key = ui_claimable_fee_amount_key(&env, &receiver, 3u32, &token);
    fx.ds.set_u128(&fx.admin, &key, &1_234u128);

    assert_eq!(fx.fh.get_ui_claimable_fee(&receiver, &3u32, &token), 1_234u128);
}

#[test]
fn test_claim_ui_fees_returns_zero_when_nothing_pending() {
    let env = Env::default();
    let fx = setup(&env);

    let receiver = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();

    let markets: Vec<u32> = vec![&env, 0u32, 1u32];
    let tokens: Vec<Address> = vec![&env, token.clone()];
    assert_eq!(fx.fh.claim_ui_fees(&receiver, &markets, &tokens), 0i128);
}

#[test]
fn test_claim_ui_fees_transfers_and_zeroes_pending() {
    let env = Env::default();
    let fx = setup(&env);

    let receiver = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let token_admin = StellarAssetClient::new(&env, &token_addr);
    let token_view = TokenClient::new(&env, &token_addr);

    // Seed two non-zero balances across two markets, both same token.
    let m_a = 0u32;
    let m_b = 1u32;
    fx.ds.set_u128(
        &fx.admin,
        &ui_claimable_fee_amount_key(&env, &receiver, m_a, &token_addr),
        &300u128,
    );
    fx.ds.set_u128(
        &fx.admin,
        &ui_claimable_fee_amount_key(&env, &receiver, m_b, &token_addr),
        &200u128,
    );
    // …and an unrelated market that should stay untouched after the claim.
    let m_other = 9u32;
    fx.ds.set_u128(
        &fx.admin,
        &ui_claimable_fee_amount_key(&env, &receiver, m_other, &token_addr),
        &50u128,
    );

    // Fund the FeeHandler contract with the underlying tokens — accrual sites
    // would have done this when they wrote the pending entries.
    token_admin.mint(&fx.fh_id, &500i128);
    assert_eq!(token_view.balance(&fx.fh_id), 500i128);
    assert_eq!(token_view.balance(&receiver), 0i128);

    let markets: Vec<u32> = vec![&env, m_a, m_b];
    let tokens: Vec<Address> = vec![&env, token_addr.clone()];
    let total = fx.fh.claim_ui_fees(&receiver, &markets, &tokens);
    assert_eq!(total, 500i128, "claim should return sum across requested markets");

    // Tokens transferred to receiver.
    assert_eq!(token_view.balance(&receiver), 500i128);
    assert_eq!(token_view.balance(&fx.fh_id), 0i128);

    // Claimed slots zeroed.
    assert_eq!(
        fx.ds
            .get_u128(&ui_claimable_fee_amount_key(&env, &receiver, m_a, &token_addr))
            .unwrap_or(0),
        0u128,
    );
    assert_eq!(
        fx.ds
            .get_u128(&ui_claimable_fee_amount_key(&env, &receiver, m_b, &token_addr))
            .unwrap_or(0),
        0u128,
    );
    // Unrequested market preserved.
    assert_eq!(
        fx.ds
            .get_u128(&ui_claimable_fee_amount_key(&env, &receiver, m_other, &token_addr))
            .unwrap_or(0),
        50u128,
    );
}

#[test]
fn test_claim_ui_fees_skips_zero_entries() {
    let env = Env::default();
    let fx = setup(&env);

    let receiver = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let token_admin = StellarAssetClient::new(&env, &token_addr);

    // Only market 2 has a balance; markets 0/1 don't. The claim must not
    // attempt a zero transfer (which the SAC would still permit, but the
    // contract's job is to skip the accounting overhead).
    fx.ds.set_u128(
        &fx.admin,
        &ui_claimable_fee_amount_key(&env, &receiver, 2u32, &token_addr),
        &75u128,
    );
    token_admin.mint(&fx.fh_id, &75i128);

    let markets: Vec<u32> = vec![&env, 0u32, 1u32, 2u32];
    let tokens: Vec<Address> = vec![&env, token_addr.clone()];
    assert_eq!(fx.fh.claim_ui_fees(&receiver, &markets, &tokens), 75i128);
}

#[test]
fn test_claim_ui_fees_handles_multiple_tokens() {
    let env = Env::default();
    let fx = setup(&env);

    let receiver = Address::generate(&env);
    let tok_a = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let tok_b = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();

    fx.ds.set_u128(
        &fx.admin,
        &ui_claimable_fee_amount_key(&env, &receiver, 0u32, &tok_a),
        &10u128,
    );
    fx.ds.set_u128(
        &fx.admin,
        &ui_claimable_fee_amount_key(&env, &receiver, 0u32, &tok_b),
        &20u128,
    );
    StellarAssetClient::new(&env, &tok_a).mint(&fx.fh_id, &10i128);
    StellarAssetClient::new(&env, &tok_b).mint(&fx.fh_id, &20i128);

    let markets: Vec<u32> = vec![&env, 0u32];
    let tokens: Vec<Address> = vec![&env, tok_a.clone(), tok_b.clone()];
    let total = fx.fh.claim_ui_fees(&receiver, &markets, &tokens);
    assert_eq!(total, 30i128);

    assert_eq!(TokenClient::new(&env, &tok_a).balance(&receiver), 10i128);
    assert_eq!(TokenClient::new(&env, &tok_b).balance(&receiver), 20i128);
}
