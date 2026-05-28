//! Tests for the FeeHandler contract (#66 + #67 + #70).
//!
//! Each claim entry point accrues against storage slots that the production
//! order/position pipelines will eventually write to. We bypass the pipelines
//! by seeding the slots directly and minting tokens to the handler — this is
//! the same end state an accrual site would leave the system in.

#![cfg(test)]

use contracts::{
    data_store::{DataStore, DataStoreClient},
    fee_handler::{FeeError, FeeHandler, FeeHandlerClient},
    keys::{
        claimable_funding_amount_key, claimable_protocol_fee_key, ui_claimable_fee_amount_key,
    },
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
    receiver: Address,
}

fn setup<'a>(env: &'a Env) -> Fx<'a> {
    env.mock_all_auths();

    let admin = Address::generate(env);

    // RoleStore is required by data_store's writer auth even though
    // FeeHandler itself does not consult it.
    let rs_id = env.register(RoleStore, ());
    RoleStoreClient::new(env, &rs_id).initialize(&admin);

    let ds_id = env.register(DataStore, ());
    let ds = DataStoreClient::new(env, &ds_id);
    ds.initialize(&admin);

    let receiver = Address::generate(env);

    let fh_id = env.register(FeeHandler, ());
    let fh = FeeHandlerClient::new(env, &fh_id);
    fh.initialize(&ds_id, &receiver);

    Fx { fh, fh_id, ds, admin, receiver }
}

// ===========================================================================
// initialize / fee receiver config
// ===========================================================================

#[test]
fn test_double_initialize_panics() {
    let env = Env::default();
    let fx = setup(&env);
    let other_ds = env.register(DataStore, ());
    let other_receiver = Address::generate(&env);
    let result = fx.fh.try_initialize(&other_ds, &other_receiver);
    assert_eq!(
        result,
        Err(Ok(soroban_sdk::Error::from_contract_error(
            FeeError::AlreadyInitialised as u32
        )))
    );
}

#[test]
fn test_get_fee_receiver_returns_initial_value() {
    let env = Env::default();
    let fx = setup(&env);
    assert_eq!(fx.fh.get_fee_receiver(), fx.receiver);
}

#[test]
fn test_set_fee_receiver_updates_value() {
    let env = Env::default();
    let fx = setup(&env);
    let new_receiver = Address::generate(&env);
    fx.fh.set_fee_receiver(&new_receiver);
    assert_eq!(fx.fh.get_fee_receiver(), new_receiver);
}

// ===========================================================================
// #70 — UI fee claim
// ===========================================================================

#[test]
fn test_get_ui_claimable_fee_defaults_zero() {
    let env = Env::default();
    let fx = setup(&env);

    let ui_receiver = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();

    assert_eq!(fx.fh.get_ui_claimable_fee(&ui_receiver, &0u32, &token), 0u128);
}

#[test]
fn test_get_ui_claimable_fee_reflects_storage() {
    let env = Env::default();
    let fx = setup(&env);

    let ui_receiver = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();

    let key = ui_claimable_fee_amount_key(&env, &ui_receiver, 3u32, &token);
    fx.ds.set_u128(&fx.admin, &key, &1_234u128);
    assert_eq!(fx.fh.get_ui_claimable_fee(&ui_receiver, &3u32, &token), 1_234u128);
}

#[test]
fn test_claim_ui_fees_returns_zero_when_nothing_pending() {
    let env = Env::default();
    let fx = setup(&env);

    let ui_receiver = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();

    let markets: Vec<u32> = vec![&env, 0u32, 1u32];
    let tokens: Vec<Address> = vec![&env, token];
    // claim_ui_fees soft no-ops with 0 when there's nothing to claim.
    assert_eq!(fx.fh.claim_ui_fees(&ui_receiver, &markets, &tokens), 0i128);
}

#[test]
fn test_claim_ui_fees_transfers_and_zeroes_pending() {
    let env = Env::default();
    let fx = setup(&env);

    let ui_receiver = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let token_admin = StellarAssetClient::new(&env, &token_addr);
    let token_view = TokenClient::new(&env, &token_addr);

    fx.ds.set_u128(
        &fx.admin,
        &ui_claimable_fee_amount_key(&env, &ui_receiver, 0u32, &token_addr),
        &300u128,
    );
    fx.ds.set_u128(
        &fx.admin,
        &ui_claimable_fee_amount_key(&env, &ui_receiver, 1u32, &token_addr),
        &200u128,
    );
    // Unrelated market — must remain untouched.
    fx.ds.set_u128(
        &fx.admin,
        &ui_claimable_fee_amount_key(&env, &ui_receiver, 9u32, &token_addr),
        &50u128,
    );
    token_admin.mint(&fx.fh_id, &500i128);

    let markets: Vec<u32> = vec![&env, 0u32, 1u32];
    let tokens: Vec<Address> = vec![&env, token_addr.clone()];
    assert_eq!(fx.fh.claim_ui_fees(&ui_receiver, &markets, &tokens), 500i128);
    assert_eq!(token_view.balance(&ui_receiver), 500i128);
    assert_eq!(token_view.balance(&fx.fh_id), 0i128);

    assert_eq!(
        fx.ds
            .get_u128(&ui_claimable_fee_amount_key(&env, &ui_receiver, 0u32, &token_addr))
            .unwrap_or(0),
        0u128,
    );
    assert_eq!(
        fx.ds
            .get_u128(&ui_claimable_fee_amount_key(&env, &ui_receiver, 1u32, &token_addr))
            .unwrap_or(0),
        0u128,
    );
    assert_eq!(
        fx.ds
            .get_u128(&ui_claimable_fee_amount_key(&env, &ui_receiver, 9u32, &token_addr))
            .unwrap_or(0),
        50u128,
    );
}

#[test]
fn test_claim_ui_fees_skips_zero_entries() {
    let env = Env::default();
    let fx = setup(&env);

    let ui_receiver = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let token_admin = StellarAssetClient::new(&env, &token_addr);

    fx.ds.set_u128(
        &fx.admin,
        &ui_claimable_fee_amount_key(&env, &ui_receiver, 2u32, &token_addr),
        &75u128,
    );
    token_admin.mint(&fx.fh_id, &75i128);

    let markets: Vec<u32> = vec![&env, 0u32, 1u32, 2u32];
    let tokens: Vec<Address> = vec![&env, token_addr.clone()];
    assert_eq!(fx.fh.claim_ui_fees(&ui_receiver, &markets, &tokens), 75i128);
}

#[test]
fn test_claim_ui_fees_handles_multiple_tokens() {
    let env = Env::default();
    let fx = setup(&env);

    let ui_receiver = Address::generate(&env);
    let tok_a = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let tok_b = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();

    fx.ds.set_u128(
        &fx.admin,
        &ui_claimable_fee_amount_key(&env, &ui_receiver, 0u32, &tok_a),
        &10u128,
    );
    fx.ds.set_u128(
        &fx.admin,
        &ui_claimable_fee_amount_key(&env, &ui_receiver, 0u32, &tok_b),
        &20u128,
    );
    StellarAssetClient::new(&env, &tok_a).mint(&fx.fh_id, &10i128);
    StellarAssetClient::new(&env, &tok_b).mint(&fx.fh_id, &20i128);

    let markets: Vec<u32> = vec![&env, 0u32];
    let tokens: Vec<Address> = vec![&env, tok_a.clone(), tok_b.clone()];
    assert_eq!(fx.fh.claim_ui_fees(&ui_receiver, &markets, &tokens), 30i128);
    assert_eq!(TokenClient::new(&env, &tok_a).balance(&ui_receiver), 10i128);
    assert_eq!(TokenClient::new(&env, &tok_b).balance(&ui_receiver), 20i128);
}

// ===========================================================================
// #66 — protocol fee claim
// ===========================================================================

#[test]
fn test_claim_fees_reverts_when_nothing_to_claim() {
    let env = Env::default();
    let fx = setup(&env);

    let token_addr = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let markets: Vec<u32> = vec![&env, 0u32, 1u32];
    let tokens: Vec<Address> = vec![&env, token_addr];

    let result = fx.fh.try_claim_fees(&markets, &tokens);
    assert_eq!(
        result,
        Err(Ok(soroban_sdk::Error::from_contract_error(
            FeeError::NothingToClaim as u32
        )))
    );
}

#[test]
fn test_claim_fees_transfers_zeroes_and_emits() {
    let env = Env::default();
    let fx = setup(&env);

    let token_addr = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let token_admin = StellarAssetClient::new(&env, &token_addr);
    let token_view = TokenClient::new(&env, &token_addr);

    let m_a = 0u32;
    let m_b = 1u32;
    fx.ds.set_u128(
        &fx.admin,
        &claimable_protocol_fee_key(&env, m_a, &token_addr),
        &300u128,
    );
    fx.ds.set_u128(
        &fx.admin,
        &claimable_protocol_fee_key(&env, m_b, &token_addr),
        &200u128,
    );
    // Untouched market — must remain after the claim.
    fx.ds.set_u128(
        &fx.admin,
        &claimable_protocol_fee_key(&env, 9u32, &token_addr),
        &50u128,
    );
    token_admin.mint(&fx.fh_id, &500i128);

    let markets: Vec<u32> = vec![&env, m_a, m_b];
    let tokens: Vec<Address> = vec![&env, token_addr.clone()];
    assert_eq!(fx.fh.claim_fees(&markets, &tokens), 500i128);

    assert_eq!(token_view.balance(&fx.receiver), 500i128);
    assert_eq!(token_view.balance(&fx.fh_id), 0i128);

    assert_eq!(
        fx.ds
            .get_u128(&claimable_protocol_fee_key(&env, m_a, &token_addr))
            .unwrap_or(0),
        0u128,
    );
    assert_eq!(
        fx.ds
            .get_u128(&claimable_protocol_fee_key(&env, m_b, &token_addr))
            .unwrap_or(0),
        0u128,
    );
    assert_eq!(
        fx.ds
            .get_u128(&claimable_protocol_fee_key(&env, 9u32, &token_addr))
            .unwrap_or(0),
        50u128,
    );
}

#[test]
fn test_claim_fees_skips_zero_slots() {
    let env = Env::default();
    let fx = setup(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let token_admin = StellarAssetClient::new(&env, &token_addr);

    fx.ds.set_u128(
        &fx.admin,
        &claimable_protocol_fee_key(&env, 2u32, &token_addr),
        &75u128,
    );
    token_admin.mint(&fx.fh_id, &75i128);

    let markets: Vec<u32> = vec![&env, 0u32, 1u32, 2u32];
    let tokens: Vec<Address> = vec![&env, token_addr.clone()];
    assert_eq!(fx.fh.claim_fees(&markets, &tokens), 75i128);
}

#[test]
fn test_get_claimable_protocol_fee_reflects_storage() {
    let env = Env::default();
    let fx = setup(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let key = claimable_protocol_fee_key(&env, 4u32, &token_addr);
    fx.ds.set_u128(&fx.admin, &key, &123u128);
    assert_eq!(fx.fh.get_claimable_protocol_fee(&4u32, &token_addr), 123u128);
    assert_eq!(fx.fh.get_claimable_protocol_fee(&999u32, &token_addr), 0u128);
}

// ===========================================================================
// #67 — per-account funding fee claim
// ===========================================================================

#[test]
fn test_claim_funding_fees_reverts_when_nothing_to_claim() {
    let env = Env::default();
    let fx = setup(&env);
    let acct = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let markets: Vec<u32> = vec![&env, 0u32];
    let tokens: Vec<Address> = vec![&env, token_addr];

    let result = fx.fh.try_claim_funding_fees(&acct, &markets, &tokens);
    assert_eq!(
        result,
        Err(Ok(soroban_sdk::Error::from_contract_error(
            FeeError::NothingToClaim as u32
        )))
    );
}

#[test]
fn test_claim_funding_fees_credits_account_and_zeroes_slot() {
    let env = Env::default();
    let fx = setup(&env);
    let acct = Address::generate(&env);
    let other = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let token_admin = StellarAssetClient::new(&env, &token_addr);
    let token_view = TokenClient::new(&env, &token_addr);

    fx.ds.set_u128(
        &fx.admin,
        &claimable_funding_amount_key(&env, 0u32, &token_addr, &acct),
        &100u128,
    );
    fx.ds.set_u128(
        &fx.admin,
        &claimable_funding_amount_key(&env, 1u32, &token_addr, &acct),
        &50u128,
    );
    fx.ds.set_u128(
        &fx.admin,
        &claimable_funding_amount_key(&env, 0u32, &token_addr, &other),
        &999u128,
    );
    token_admin.mint(&fx.fh_id, &150i128);

    let markets: Vec<u32> = vec![&env, 0u32, 1u32];
    let tokens: Vec<Address> = vec![&env, token_addr.clone()];
    assert_eq!(fx.fh.claim_funding_fees(&acct, &markets, &tokens), 150i128);

    assert_eq!(token_view.balance(&acct), 150i128);
    assert_eq!(
        fx.ds
            .get_u128(&claimable_funding_amount_key(&env, 0u32, &token_addr, &acct))
            .unwrap_or(0),
        0u128,
    );
    assert_eq!(
        fx.ds
            .get_u128(&claimable_funding_amount_key(&env, 1u32, &token_addr, &acct))
            .unwrap_or(0),
        0u128,
    );
    assert_eq!(
        fx.ds
            .get_u128(&claimable_funding_amount_key(&env, 0u32, &token_addr, &other))
            .unwrap_or(0),
        999u128,
    );
}

#[test]
fn test_get_claimable_funding_reflects_storage() {
    let env = Env::default();
    let fx = setup(&env);
    let acct = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    let key = claimable_funding_amount_key(&env, 7u32, &token_addr, &acct);
    fx.ds.set_u128(&fx.admin, &key, &42u128);
    assert_eq!(fx.fh.get_claimable_funding(&acct, &7u32, &token_addr), 42u128);
    let other = Address::generate(&env);
    assert_eq!(fx.fh.get_claimable_funding(&other, &7u32, &token_addr), 0u128);
}

#[test]
fn test_claim_funding_fees_requires_account_auth() {
    // mock_all_auths() lets every required_auth() succeed; the real check is
    // that the account appears as the authorizer on its own invocation.
    let env = Env::default();
    let fx = setup(&env);
    let acct = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(fx.admin.clone())
        .address();
    fx.ds.set_u128(
        &fx.admin,
        &claimable_funding_amount_key(&env, 0u32, &token_addr, &acct),
        &10u128,
    );
    StellarAssetClient::new(&env, &token_addr).mint(&fx.fh_id, &10i128);

    let markets: Vec<u32> = vec![&env, 0u32];
    let tokens: Vec<Address> = vec![&env, token_addr.clone()];
    fx.fh.claim_funding_fees(&acct, &markets, &tokens);

    let auths = env.auths();
    let function_name = soroban_sdk::Symbol::new(&env, "claim_funding_fees");
    let saw_account_signing = auths.iter().any(|(addr, invocation)| {
        if addr != &acct {
            return false;
        }
        match &invocation.function {
            soroban_sdk::testutils::AuthorizedFunction::Contract((_, name, _)) => {
                *name == function_name
            }
            _ => false,
        }
    });
    assert!(
        saw_account_signing,
        "claim_funding_fees should require the account's auth on its own invocation: {:?}",
        auths
    );
}
