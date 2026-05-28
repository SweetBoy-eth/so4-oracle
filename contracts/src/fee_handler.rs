//! Fee handler: protocol/UI/funding fee claim entry points (#66/#67/#70).
//!
//! This PR introduces the **UI fee** side of the handler (#70). Protocol-fee
//! and per-account funding-fee claims (#66/#67) live in the kalveen branch.
//!
//! Storage layout: `ui_claimable_fee_amount_key(receiver, market_id, token)`
//! holds the per-receiver / market / token balance pending a claim. The pool
//! holding the underlying tokens is the `FeeHandler` contract itself: protocol
//! flows that accrue UI fees transfer the tokens to this contract and write the
//! accounting entry via the data store. (Wiring those accrual sites into the
//! position fee computation is the production follow-up referenced in #70.)

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
};

use crate::{data_store::DataStoreClient, keys::ui_claimable_fee_amount_key};

#[contract]
pub struct FeeHandler;

#[contracttype]
enum FeeHandlerKey {
    DataStore,
}

#[contractimpl]
impl FeeHandler {
    pub fn initialize(env: Env, data_store: Address) {
        if env.storage().instance().has(&FeeHandlerKey::DataStore) {
            panic!("already initialised");
        }
        env.storage()
            .instance()
            .set(&FeeHandlerKey::DataStore, &data_store);
    }

    /// Claim UI fees the caller (`ui_fee_receiver`) has accrued across the
    /// provided `markets` / `tokens` cross-product. Returns the total amount
    /// transferred, zeroes out each non-zero entry, and emits a
    /// `(uifee_clm, token)` event per market for off-chain accounting.
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

    fn data_store(env: &Env) -> DataStoreClient<'_> {
        let addr: Address = env
            .storage()
            .instance()
            .get(&FeeHandlerKey::DataStore)
            .expect("not initialised");
        DataStoreClient::new(env, &addr)
    }
}
