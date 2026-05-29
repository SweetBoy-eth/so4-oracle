//! Tests for the DepositVault contract.

#![cfg(test)]

use contracts::{
    deposit_vault::{DepositVault, DepositVaultClient},
    role_store::{RoleStore, RoleStoreClient},
};
use soroban_sdk::{
    contract, contractimpl,
    testutils::Address as _,
    token::{StellarAssetClient, TokenClient},
    Address, Env, IntoVal, InvokeError, Symbol, Vec,
};

#[contract]
pub struct ExecuteDepositInvoker;

#[contractimpl]
impl ExecuteDepositInvoker {
    pub fn invoke_execute_deposit(
        env: Env,
        contract: Address,
        caller: Address,
        deposit_id: u32,
        oracle_price: u128,
    ) {
        let vault = DepositVaultClient::new(&env, &contract);
        vault.execute_vault_deposit(&caller, &deposit_id, &oracle_price);
    }
}

struct Setup {
    vault: Address,
    admin: Address,
    controller: Address,
    user: Address,
    receiver: Address,
    token: Address,
}

fn setup(env: &Env) -> Setup {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let controller = Address::generate(env);
    let user = Address::generate(env);
    let receiver = Address::generate(env);

    let rs_id = env.register(RoleStore, ());
    let rs = RoleStoreClient::new(env, &rs_id);
    rs.initialize(&admin);

    let vault_id = env.register(DepositVault, ());
    let vault = DepositVaultClient::new(env, &vault_id);
    vault.initialize(&rs_id, &controller);

    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    StellarAssetClient::new(env, &token).mint(&user, &1000i128);

    Setup {
        vault: vault_id,
        admin,
        controller,
        user,
        receiver,
        token,
    }
}

fn balance(env: &Env, token: &Address, account: &Address) -> i128 {
    TokenClient::new(env, token).balance(account)
}

#[test]
fn test_failed_execute_leaves_vault_balance_intact_and_cancel_refunds_exact_amount() {
    let env = Env::default();
    let s = setup(&env);
    let vault_client = DepositVaultClient::new(&env, &s.vault);

    let deposit_id = vault_client.create_deposit(&s.user, &s.token, &500u128);

    assert_eq!(balance(&env, &s.token, &s.user), 500);
    assert_eq!(balance(&env, &s.token, &s.vault), 500);
    assert_eq!(vault_client.recorded_balance(&s.token), 500);

    let invoker_id = env.register(ExecuteDepositInvoker, ());
    let args = Vec::from_array(
        &env,
        [
            s.controller.clone().into_val(&env),
            deposit_id.into_val(&env),
            0u128.into_val(&env),
        ],
    );
    let result = env.try_invoke_contract::<(), InvokeError>(
        &invoker_id,
        &Symbol::new(&env, "invoke_execute_deposit"),
        args,
    );
    assert!(result.is_err(), "failed execute_deposit should revert");

    assert_eq!(balance(&env, &s.token, &s.user), 500);
    assert_eq!(balance(&env, &s.token, &s.vault), 500);
    assert_eq!(vault_client.recorded_balance(&s.token), 500);

    vault_client.cancel_deposit(&s.user, &deposit_id);

    assert_eq!(balance(&env, &s.token, &s.user), 1000);
    assert_eq!(balance(&env, &s.token, &s.vault), 0);
    assert_eq!(vault_client.recorded_balance(&s.token), 0);
}

#[test]
fn test_emergency_drain_only_admin_can_call() {
    let env = Env::default();
    let s = setup(&env);
    let vault_client = DepositVaultClient::new(&env, &s.vault);

    let deposit_id = vault_client.create_deposit(&s.user, &s.token, &200u128);
    assert_eq!(vault_client.recorded_balance(&s.token), 200);
    assert_eq!(balance(&env, &s.token, &s.vault), 200);

    vault_client.emergency_drain(&s.admin, &s.token, &s.receiver);

    assert_eq!(balance(&env, &s.token, &s.receiver), 200);
    assert_eq!(vault_client.recorded_balance(&s.token), 0);

    let args = Vec::from_array(
        &env,
        [
            s.user.clone().into_val(&env),
            s.token.clone().into_val(&env),
            s.receiver.clone().into_val(&env),
        ],
    );
    let result = env.try_invoke_contract::<(), InvokeError>(
        &s.vault,
        &Symbol::new(&env, "emergency_drain"),
        args,
    );
    assert!(result.is_err(), "non-admin emergency_drain must revert");
}
