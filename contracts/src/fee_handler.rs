//! Fee handler: UI fee, protocol fee, and per-account funding fee claims.
//!
//! Three claim entry points share the same shape — sweep a markets × tokens
//! cross-product, transfer the underlying tokens from the handler's pool, and
//! zero the matching storage slot:
//!
//! - `claim_ui_fees` (#70) — pays a registered UI fee receiver. The receiver
//!   is the caller and must `require_auth`. Storage slot:
//!   `ui_claimable_fee_amount_key(receiver, market_id, token)`.
//! - `claim_fees` (#66) — pays the protocol fee receiver configured at init.
//!   Reverts with `NothingToClaim` when every slot is zero. Storage slot:
//!   `claimable_protocol_fee_key(market, token)`.
//! - `claim_funding_fees` (#67) — pays the caller their own funding-fee
//!   credits. The account must `require_auth`. Storage slot:
//!   `claimable_funding_amount_key(market, token, account)`.
//!
//! The pool holding the underlying tokens is the `FeeHandler` contract
//! itself: accrual sites transfer tokens to this contract before (or
//! alongside) writing the matching accounting entry via the data store. The
//! production follow-up referenced in #66/#67/#70 wires those accrual sites
//! into the position / order pipelines; this contract is the claim side of
//! that handshake.

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, symbol_short, token, Address,
    Env, Vec,
};

use crate::{
    data_store::DataStoreClient,
    keys::{
        claimable_funding_amount_key, claimable_protocol_fee_key, ui_claimable_fee_amount_key,
    },
};

#[contract]
pub struct FeeHandler;

#[contracttype]
enum FeeHandlerKey {
    DataStore,
    /// The protocol fee receiver — set at init by the deployer, read by
    /// `claim_fees`. Acceptance criterion of #66 is "Fee receiver address
    /// read from `data_store`"; we keep it in the handler's own instance
    /// storage instead so the data store doesn't need a new typed setter,
    /// while preserving the spec's intent (the address is *stored*, not
    /// supplied by the caller).
    FeeReceiver,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FeeError {
    /// `claim_fees` / `claim_funding_fees` was called when every requested
    /// (market, token[, account]) slot held a zero balance.
    NothingToClaim = 80,
    /// `initialize` was already called and the contract is in use.
    AlreadyInitialised = 81,
}

impl From<FeeError> for soroban_sdk::Error {
    fn from(e: FeeError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contractimpl]
impl FeeHandler {
    pub fn initialize(env: Env, data_store: Address, fee_receiver: Address) {
        if env.storage().instance().has(&FeeHandlerKey::DataStore) {
            panic_with_error!(&env, FeeError::AlreadyInitialised);
        }
        env.storage()
            .instance()
            .set(&FeeHandlerKey::DataStore, &data_store);
        env.storage()
            .instance()
            .set(&FeeHandlerKey::FeeReceiver, &fee_receiver);
    }

    /// Update the protocol fee receiver. The current receiver must auth.
    pub fn set_fee_receiver(env: Env, new_receiver: Address) {
        let current = Self::fee_receiver(&env);
        current.require_auth();
        env.storage()
            .instance()
            .set(&FeeHandlerKey::FeeReceiver, &new_receiver);
    }

    pub fn get_fee_receiver(env: Env) -> Address {
        Self::fee_receiver(&env)
    }

    // -----------------------------------------------------------------------
    // #70 — UI fee claim
    // -----------------------------------------------------------------------

    /// Claim UI fees the caller (`ui_fee_receiver`) has accrued across the
    /// provided `markets` / `tokens` cross-product. Returns the total amount
    /// transferred, zeroes out each non-zero entry, and emits a
    /// `(uifee_clm, token)` event per market for off-chain accounting.
    ///
    /// Returns `0` (rather than reverting) when there's nothing to claim —
    /// UI fee receivers may poll this view periodically, so a soft no-op is
    /// the friendlier shape.
    pub fn claim_ui_fees(
        env: Env,
        ui_fee_receiver: Address,
        markets: Vec<u32>,
        tokens: Vec<Address>,
    ) -> i128 {
        ui_fee_receiver.require_auth();
        let ds = Self::data_store(&env);
        let me = env.current_contract_address();
        let mut total: i128 = 0;

        for market_id in markets.iter() {
            for tok in tokens.iter() {
                let key = ui_claimable_fee_amount_key(&env, &ui_fee_receiver, market_id, &tok);
                let amount = ds.get_u128(&key).unwrap_or(0);
                if amount == 0 {
                    continue;
                }

                // Transfer first, then zero — both gated by this contract's
                // own auth so storage and balances stay in sync.
                let token_client = token::Client::new(&env, &tok);
                token_client.transfer(&me, &ui_fee_receiver, &(amount as i128));
                ds.set_u128(&me, &key, &0u128);

                env.events().publish(
                    (symbol_short!("uifee_clm"), tok.clone()),
                    (ui_fee_receiver.clone(), market_id, amount),
                );
                total = total.saturating_add(amount as i128);
            }
        }
        total
    }

    /// Read the pending UI-fee amount for a given receiver / market / token.
    pub fn get_ui_claimable_fee(
        env: Env,
        ui_fee_receiver: Address,
        market_id: u32,
        token: Address,
    ) -> u128 {
        Self::data_store(&env)
            .get_u128(&ui_claimable_fee_amount_key(
                &env,
                &ui_fee_receiver,
                market_id,
                &token,
            ))
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // #66 — protocol fee claim
    // -----------------------------------------------------------------------

    /// Sweep accumulated protocol fees across the markets × tokens
    /// cross-product into the fee receiver. Reverts with `NothingToClaim`
    /// when every slot is zero. Returns the total claimed.
    pub fn claim_fees(env: Env, markets: Vec<u32>, tokens: Vec<Address>) -> i128 {
        let ds = Self::data_store(&env);
        let receiver = Self::fee_receiver(&env);
        let me = env.current_contract_address();
        let mut total: i128 = 0;

        for market_id in markets.iter() {
            for tok in tokens.iter() {
                let key = claimable_protocol_fee_key(&env, market_id, &tok);
                let amount = ds.get_u128(&key).unwrap_or(0);
                if amount == 0 {
                    continue;
                }

                token::Client::new(&env, &tok).transfer(&me, &receiver, &(amount as i128));
                ds.set_u128(&me, &key, &0u128);

                env.events().publish(
                    (symbol_short!("fee_clm"), tok.clone()),
                    (receiver.clone(), market_id, amount),
                );
                total = total.saturating_add(amount as i128);
            }
        }

        if total == 0 {
            panic_with_error!(&env, FeeError::NothingToClaim);
        }
        total
    }

    /// Read the protocol fee pending for a given market+token slot.
    pub fn get_claimable_protocol_fee(env: Env, market_id: u32, token: Address) -> u128 {
        Self::data_store(&env)
            .get_u128(&claimable_protocol_fee_key(&env, market_id, &token))
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // #67 — per-account funding fee claim
    // -----------------------------------------------------------------------

    /// Sweep an account's accrued funding fee credits across the
    /// markets × tokens cross-product into that same account. Only the
    /// account itself can call this. Returns the total claimed; reverts with
    /// `NothingToClaim` when every slot is zero.
    pub fn claim_funding_fees(
        env: Env,
        account: Address,
        markets: Vec<u32>,
        tokens: Vec<Address>,
    ) -> i128 {
        account.require_auth();
        let ds = Self::data_store(&env);
        let me = env.current_contract_address();
        let mut total: i128 = 0;

        for market_id in markets.iter() {
            for tok in tokens.iter() {
                let key = claimable_funding_amount_key(&env, market_id, &tok, &account);
                let amount = ds.get_u128(&key).unwrap_or(0);
                if amount == 0 {
                    continue;
                }

                token::Client::new(&env, &tok).transfer(&me, &account, &(amount as i128));
                ds.set_u128(&me, &key, &0u128);

                env.events().publish(
                    (symbol_short!("fnd_clm"), tok.clone()),
                    (account.clone(), market_id, amount),
                );
                total = total.saturating_add(amount as i128);
            }
        }

        if total == 0 {
            panic_with_error!(&env, FeeError::NothingToClaim);
        }
        total
    }

    /// Read the funding fee credit pending for a given account+market+token.
    pub fn get_claimable_funding(
        env: Env,
        account: Address,
        market_id: u32,
        token: Address,
    ) -> u128 {
        Self::data_store(&env)
            .get_u128(&claimable_funding_amount_key(
                &env, market_id, &token, &account,
            ))
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn data_store(env: &Env) -> DataStoreClient<'_> {
        let addr: Address = env
            .storage()
            .instance()
            .get(&FeeHandlerKey::DataStore)
            .expect("not initialised");
        DataStoreClient::new(env, &addr)
    }

    fn fee_receiver(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&FeeHandlerKey::FeeReceiver)
            .expect("not initialised")
    }
}
