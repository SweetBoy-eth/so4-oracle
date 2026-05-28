use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Vec};

use crate::{
    data_store::DataStoreClient,
    keys::{
        borrowing_factor_key, funding_factor_key, impact_pool_amount_key,
        market_maintenance_margin_factor_key, open_interest_long_key,
        open_interest_short_key, position_fee_factor_key,
        price_impact_exponent_factor_key, price_impact_factor_key,
    },
    liquidity_handler::LiquidityHandlerClient,
    market_utils,
    order_handler::OrderHandlerClient,
    position_utils::{
        calculate_pnl, get_position_fees, get_position_liquidation_price,
        get_position_pnl_usd,
    },
    pricing_utils::{get_execution_price as compute_execution_price, FACTOR_DENOMINATOR},
    referral_storage::ReferralStorageClient,
    types::{
        ExecutionPriceResult, FundingInfo, Order, PoolValueInfo, PositionFees, PositionInfo,
        PositionFundingFactors, PositionProps, ReferrerStats, Withdrawal,
    },
};

#[contract]
pub struct Reader;

#[contracttype]
enum ReaderKey {
    DataStore,
    LiquidityHandler,
}

/// ADL target entry: (account, position_key, unrealised_pnl_usd).
pub type AdlTarget = (Address, BytesN<32>, i128);

#[contractimpl]
impl Reader {
    pub fn initialize(env: Env, data_store: Address, liquidity_handler: Address) {
        if env.storage().instance().has(&ReaderKey::DataStore) {
            panic!("already initialised");
        }
        env.storage().instance().set(&ReaderKey::DataStore, &data_store);
        env.storage().instance().set(&ReaderKey::LiquidityHandler, &liquidity_handler);
    }

    /// Returns the top-`count` most profitable open positions for `market_id`
    /// on the given side (`is_long`), sorted by `unrealised_pnl_usd` descending.
    ///
    /// Each entry is `(account, position_key, unrealised_pnl_usd)`.
    pub fn get_adl_targets(
        env: Env,
        market_id: u32,
        is_long: bool,
        count: u32,
    ) -> Vec<AdlTarget> {
        let ds = Self::data_store(&env);
        let lh = Self::liquidity_handler(&env);

        let prices = lh.oracle_prices(&market_id);
        let current_price = if is_long {
            prices.long_price
        } else {
            prices.short_price
        };

        let positions: Vec<PositionProps> =
            ds.get_all_positions_for_market(&market_id, &is_long, &0, &u32::MAX);

        let mut entries: Vec<AdlTarget> = Vec::new(&env);
        for pos in positions.iter() {
            let pnl = calculate_pnl(&pos, current_price);
            entries.push_back((pos.account.clone(), pos.position_key.clone(), pnl));
        }

        // Sort by PnL descending using insertion sort (no_std compatible).
        let len = entries.len();
        for i in 1..len {
            let current = entries.get(i).unwrap();
            let mut j = i;
            while j > 0 {
                let prev = entries.get(j - 1).unwrap();
                if prev.2 >= current.2 {
                    break;
                }
                entries.set(j, prev);
                j -= 1;
            }
            entries.set(j, current);
        }

        // Truncate to `count` entries.
        let limit = if count < entries.len() { count } else { entries.len() };
        let mut result: Vec<AdlTarget> = Vec::new(&env);
        for i in 0..limit {
            result.push_back(entries.get(i).unwrap());
        }
        result
    }

    /// Preview the execution price for `size_delta_usd` on the given position,
    /// including OI-based price impact. Returns prices with and without impact.
    pub fn get_execution_price(
        env: Env,
        position_key: BytesN<32>,
        size_delta_usd: u128,
        is_increase: bool,
    ) -> ExecutionPriceResult {
        let ds = Self::data_store(&env);
        let lh = Self::liquidity_handler(&env);

        let pos: PositionProps = ds
            .get_position_props(&position_key)
            .expect("position not found");

        let prices = lh.oracle_prices(&pos.market_id);
        let index_price = if pos.is_long {
            prices.long_price
        } else {
            prices.short_price
        };

        let long_oi = ds
            .get_u128(&open_interest_long_key(&env, pos.market_id))
            .unwrap_or(0);
        let short_oi = ds
            .get_u128(&open_interest_short_key(&env, pos.market_id))
            .unwrap_or(0);
        let impact_factor = ds
            .get_u128(&price_impact_factor_key(&env, pos.market_id))
            .unwrap_or(0);
        // Unset exponent defaults to `^1` (a linear curve).
        let impact_exponent_factor = ds
            .get_u128(&price_impact_exponent_factor_key(&env, pos.market_id))
            .unwrap_or(FACTOR_DENOMINATOR);
        let impact_pool_amount = ds
            .get_u128(&impact_pool_amount_key(&env, pos.market_id))
            .unwrap_or(0);

        let result = compute_execution_price(
            index_price,
            size_delta_usd,
            long_oi,
            short_oi,
            pos.is_long,
            is_increase,
            impact_factor,
            impact_exponent_factor,
            impact_pool_amount,
        );

        ExecutionPriceResult {
            price_without_impact: result.price_without_impact,
            price_with_impact: result.price_with_impact,
        }
    }

    pub fn get_position_info(
        env: Env,
        position_key: BytesN<32>,
        maximize: bool,
    ) -> PositionInfo {
        let ds = Self::data_store(&env);
        let lh = Self::liquidity_handler(&env);

        let mut pos: PositionProps = ds
            .get_position_props(&position_key)
            .expect("position not found");

        let prices = lh.oracle_prices(&pos.market_id);
        let current_price = if pos.is_long {
            if maximize {
                prices.long_price.min(prices.short_price)
            } else {
                prices.long_price.max(prices.short_price)
            }
        } else if maximize {
            prices.long_price.max(prices.short_price)
        } else {
            prices.long_price.min(prices.short_price)
        };

        let pnl_usd = get_position_pnl_usd(&pos, current_price);

        let funding_factor = ds
            .get_u128(&funding_factor_key(&env, pos.market_id))
            .unwrap_or(0);
        let borrowing_factor = ds
            .get_u128(&borrowing_factor_key(&env, pos.market_id))
            .unwrap_or(0);
        let position_fee_factor = ds
            .get_u128(&position_fee_factor_key(&env, pos.market_id))
            .unwrap_or(0);
        let maintenance_margin_factor = ds
            .get_u128(&market_maintenance_margin_factor_key(&env, pos.market_id))
            .unwrap_or(0);

        let (funding_fee, borrowing_fee, position_fee, total_fee) = get_position_fees(
            pos.quantity,
            funding_factor,
            borrowing_factor,
            position_fee_factor,
        );

        let pending_fees = PositionFees {
            borrowing_fee,
            funding_fee,
            position_fee,
            total_fee,
        };

        let liquidation_price = get_position_liquidation_price(
            &pos,
            maintenance_margin_factor,
            funding_factor,
            borrowing_factor,
            position_fee_factor,
        );

        let funding_info = PositionFundingFactors {
            borrowing_factor,
            funding_factor,
            position_fee_factor,
        };

        PositionInfo {
            position: pos,
            pnl_usd,
            pending_fees,
            liquidation_price,
            funding_info,
        }
    }

    pub fn get_market_pool_value_info(
        env: Env,
        market_id: u32,
        long_price: u128,
        short_price: u128,
        maximize: bool,
    ) -> PoolValueInfo {
        let ds = Self::data_store(&env);
        let lh = Self::liquidity_handler(&env);

        let (pool_long, pool_short) = lh.pool_amounts(&market_id);
        let impact_pool_amount = ds
            .get_u128(&impact_pool_amount_key(&env, market_id))
            .unwrap_or(0);
        let lp_supply = lh.lp_supply(&market_id);

        market_utils::get_pool_value(
            pool_long,
            pool_short,
            long_price,
            short_price,
            impact_pool_amount,
            lp_supply,
            maximize,
        )
    }

    // -----------------------------------------------------------------------
    // Funding view (#73)
    // -----------------------------------------------------------------------

    /// Snapshot of a market's funding state: signed funding factor per second
    /// plus current open interest on each side. Returns zeros where storage
    /// has not been written yet, so callers can render the panel before any
    /// trading has occurred.
    pub fn get_funding_info(env: Env, market_id: u32) -> FundingInfo {
        let ds = Self::data_store(&env);
        let funding_factor_per_second = ds
            .get_i128(&funding_factor_key(&env, market_id))
            .unwrap_or(0);
        let open_interest_long = ds
            .get_u128(&open_interest_long_key(&env, market_id))
            .unwrap_or(0);
        let open_interest_short = ds
            .get_u128(&open_interest_short_key(&env, market_id))
            .unwrap_or(0);
        // Per-side aggregate claimable funding totals are not tracked at the
        // protocol level today (claimable funding is keyed per account+token
        // via #67's `claimable_funding_amount_key`). Surface 0 here so the
        // struct shape matches the issue spec.
        FundingInfo {
            funding_factor_per_second,
            open_interest_long,
            open_interest_short,
            claimable_funding_long: 0,
            claimable_funding_short: 0,
        }
    }

    // -----------------------------------------------------------------------
    // Order views (#71)
    // -----------------------------------------------------------------------

    /// Pending orders belonging to `account`, ordered by creation and
    /// paginated with `start` (skip) and `limit` (max returned). Cancelled and
    /// executed orders are automatically excluded — the underlying storage
    /// removes them, so iterating returns only the pending set.
    pub fn get_account_orders(
        env: Env,
        order_handler: Address,
        account: Address,
        start: u32,
        limit: u32,
    ) -> Vec<Order> {
        let mut out = Vec::new(&env);
        if limit == 0 {
            return out;
        }
        let oh = OrderHandlerClient::new(&env, &order_handler);
        let count = oh.get_order_count();
        let mut skipped: u32 = 0;
        for id in 0..count {
            let Some(order) = oh.get_order(&id) else { continue };
            if order.account != account {
                continue;
            }
            if skipped < start {
                skipped += 1;
                continue;
            }
            out.push_back(order);
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    // -----------------------------------------------------------------------
    // Withdrawal views (#72)
    // -----------------------------------------------------------------------
    //
    // Pending withdrawals are persisted by the LiquidityHandler; deposits are
    // executed atomically (LP shares are minted immediately) and have no
    // pending record to surface — the matching `get_deposit` / account-deposit
    // views are intentionally not implemented here, see the PR body for the
    // storage change that would be required.

    pub fn get_withdrawal(env: Env, withdrawal_id: u32) -> Option<Withdrawal> {
        Self::liquidity_handler(&env).get_withdrawal(&withdrawal_id)
    }

    pub fn get_account_withdrawals(
        env: Env,
        account: Address,
        start: u32,
        limit: u32,
    ) -> Vec<Withdrawal> {
        let mut out = Vec::new(&env);
        if limit == 0 {
            return out;
        }
        let lh = Self::liquidity_handler(&env);
        let count = lh.get_withdrawal_count();
        let mut skipped: u32 = 0;
        for id in 0..count {
            let Some(w) = lh.get_withdrawal(&id) else { continue };
            if w.account != account {
                continue;
            }
            if skipped < start {
                skipped += 1;
                continue;
            }
            out.push_back(w);
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    // -----------------------------------------------------------------------
    // Open interest views (#74)
    // -----------------------------------------------------------------------

    /// USD-valued open interest per side for `market_id`. The underlying
    /// `open_interest_long_key` / `open_interest_short_key` slots are already
    /// written in USD by the position pipeline (see `increase_position_utils`
    /// where `size_delta_usd` is accumulated), so this view is a direct read.
    pub fn get_open_interest(env: Env, market_id: u32) -> (u128, u128) {
        let ds = Self::data_store(&env);
        let long_oi = ds
            .get_u128(&open_interest_long_key(&env, market_id))
            .unwrap_or(0);
        let short_oi = ds
            .get_u128(&open_interest_short_key(&env, market_id))
            .unwrap_or(0);
        (long_oi, short_oi)
    }

    /// Open interest expressed in pool-token units (long_oi / long_price,
    /// short_oi / short_price). Returns (0, 0) if oracle prices are unset for
    /// the market, so the call never panics for an unconfigured market —
    /// callers can render a blank panel before any trading has occurred.
    pub fn get_open_interest_in_tokens(env: Env, market_id: u32) -> (u128, u128) {
        let ds = Self::data_store(&env);
        let lh = Self::liquidity_handler(&env);

        let long_oi = ds
            .get_u128(&open_interest_long_key(&env, market_id))
            .unwrap_or(0);
        let short_oi = ds
            .get_u128(&open_interest_short_key(&env, market_id))
            .unwrap_or(0);
        // `oracle_prices` panics if no prices have been set — use the `try_*`
        // variant so this view stays panic-free for unconfigured markets.
        let prices = match lh.try_oracle_prices(&market_id) {
            Ok(Ok(p)) => p,
            _ => return (0, 0),
        };
        let long_in_tokens = if prices.long_price == 0 {
            0
        } else {
            long_oi / prices.long_price
        };
        let short_in_tokens = if prices.short_price == 0 {
            0
        } else {
            short_oi / prices.short_price
        };
        (long_in_tokens, short_in_tokens)
    }

    // -----------------------------------------------------------------------
    // Referral stats view (#69)
    // -----------------------------------------------------------------------

    /// Returns the cumulative stats tracked by `ReferralStorage` for a
    /// referrer. Zero-stats are returned if the referrer has never been
    /// recorded — same shape and semantics as
    /// `ReferralStorage::get_referrer_stats`, exposed here so UIs can hit a
    /// single read contract.
    pub fn get_referrer_stats(
        env: Env,
        referral_storage: Address,
        referrer: Address,
    ) -> ReferrerStats {
        ReferralStorageClient::new(&env, &referral_storage).get_referrer_stats(&referrer)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn data_store(env: &Env) -> DataStoreClient<'_> {
        let addr: Address = env
            .storage()
            .instance()
            .get(&ReaderKey::DataStore)
            .expect("not initialised");
        DataStoreClient::new(env, &addr)
    }

    fn liquidity_handler(env: &Env) -> LiquidityHandlerClient<'_> {
        let addr: Address = env
            .storage()
            .instance()
            .get(&ReaderKey::LiquidityHandler)
            .expect("not initialised");
        LiquidityHandlerClient::new(env, &addr)
    }
}
