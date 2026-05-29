use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, token, Address, Env};

use crate::role_store::{role_admin_id, RoleStoreClient};

#[contract]
pub struct DepositVault;

#[contracttype]
enum VaultKey {
    RoleStore,
    Controller,
    Deposit(u32),
    DepositCount,
    RecordedBalance(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepositRecord {
    pub depositor: Address,
    pub token: Address,
    pub amount: u128,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum VaultError {
    Unauthorized = 1,
    DepositNotFound = 2,
    InvalidAmount = 3,
    NotDepositor = 4,
    MissingOraclePrice = 5,
}

impl From<VaultError> for soroban_sdk::Error {
    fn from(e: VaultError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contractimpl]
impl DepositVault {
    pub fn initialize(env: Env, role_store: Address, controller: Address) {
        if env.storage().instance().has(&VaultKey::RoleStore) {
            panic!("already initialised");
        }
        env.storage()
            .instance()
            .set(&VaultKey::RoleStore, &role_store);
        env.storage()
            .instance()
            .set(&VaultKey::Controller, &controller);
    }

    pub fn create_deposit(env: Env, caller: Address, token: Address, amount: u128) -> u32 {
        caller.require_auth();
        if amount == 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }

        token::TokenClient::new(&env, &token).transfer(
            &caller,
            &env.current_contract_address(),
            &(amount as i128),
        );

        let deposit_id = Self::next_deposit_id(&env);
        let record = DepositRecord {
            depositor: caller.clone(),
            token: token.clone(),
            amount,
        };
        env.storage()
            .persistent()
            .set(&VaultKey::Deposit(deposit_id), &record);
        env.storage()
            .persistent()
            .set(&VaultKey::DepositCount, &(deposit_id + 1));

        let current_balance = Self::recorded_balance(env.clone(), token.clone());
        env.storage().persistent().set(
            &VaultKey::RecordedBalance(token),
            &(current_balance.saturating_add(amount)),
        );

        deposit_id
    }

    pub fn execute_vault_deposit(env: Env, caller: Address, deposit_id: u32, oracle_price: u128) {
        caller.require_auth();
        Self::require_controller(&env, &caller);

        if oracle_price == 0 {
            panic_with_error!(&env, VaultError::MissingOraclePrice);
        }

        let record: DepositRecord = match env
            .storage()
            .persistent()
            .get(&VaultKey::Deposit(deposit_id))
        {
            Some(r) => r,
            None => panic_with_error!(&env, VaultError::DepositNotFound),
        };

        env.storage()
            .persistent()
            .remove(&VaultKey::Deposit(deposit_id));

        let token = record.token;
        let current_balance = Self::recorded_balance(env.clone(), token.clone());
        env.storage().persistent().set(
            &VaultKey::RecordedBalance(token),
            &current_balance.saturating_sub(record.amount),
        );
    }

    pub fn cancel_deposit(env: Env, caller: Address, deposit_id: u32) {
        caller.require_auth();

        let record: DepositRecord = match env
            .storage()
            .persistent()
            .get(&VaultKey::Deposit(deposit_id))
        {
            Some(r) => r,
            None => panic_with_error!(&env, VaultError::DepositNotFound),
        };

        if caller != record.depositor {
            panic_with_error!(&env, VaultError::NotDepositor);
        }

        let token = record.token.clone();
        let amount = record.amount;

        env.storage()
            .persistent()
            .remove(&VaultKey::Deposit(deposit_id));

        let current_balance = Self::recorded_balance(env.clone(), token.clone());
        env.storage().persistent().set(
            &VaultKey::RecordedBalance(token.clone()),
            &current_balance.saturating_sub(amount),
        );

        Self::transfer_out(&env, &token, &record.depositor, amount);
    }

    pub fn emergency_drain(env: Env, caller: Address, token: Address, receiver: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let amount = Self::recorded_balance(env.clone(), token.clone());
        if amount == 0 {
            return;
        }

        Self::transfer_out(&env, &token, &receiver, amount);
        env.storage()
            .persistent()
            .set(&VaultKey::RecordedBalance(token.clone()), &0u128);
        env.events()
            .publish(("emergency_drain",), (token, receiver, amount));
    }

    pub fn recorded_balance(env: Env, token: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&VaultKey::RecordedBalance(token))
            .unwrap_or(0u128)
    }

    pub fn get_deposit(env: Env, deposit_id: u32) -> Option<DepositRecord> {
        env.storage()
            .persistent()
            .get(&VaultKey::Deposit(deposit_id))
    }

    fn next_deposit_id(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&VaultKey::DepositCount)
            .unwrap_or(0u32)
    }

    fn require_controller(env: &Env, caller: &Address) {
        let controller: Address = env
            .storage()
            .instance()
            .get(&VaultKey::Controller)
            .expect("not initialised");
        if caller != &controller {
            panic_with_error!(env, VaultError::Unauthorized);
        }
    }

    fn require_admin(env: &Env, caller: &Address) {
        let rs_addr: Address = env
            .storage()
            .instance()
            .get(&VaultKey::RoleStore)
            .expect("not initialised");
        let rs = RoleStoreClient::new(env, &rs_addr);
        let is_admin = rs.has_role(&role_admin_id(env), caller);
        if !is_admin {
            panic_with_error!(env, VaultError::Unauthorized);
        }
    }

    fn transfer_out(env: &Env, token: &Address, receiver: &Address, amount: u128) {
        if amount == 0 {
            return;
        }
        token::TokenClient::new(env, token).transfer(
            &env.current_contract_address(),
            receiver,
            &(amount as i128),
        );
    }
}
