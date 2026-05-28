use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, Address, BytesN, Env};

use crate::{
    role_store::{role_admin_id, RoleStoreClient},
    types::{ReferralError, ReferrerStats, TierConfig},
};

#[contract]
pub struct ReferralStorage;

#[contracttype]
enum ReferralStorageKey {
    RoleStore,
    CodeOwner(BytesN<32>),
    Tier(Address),
    /// Per-referrer cumulative stats (#69).
    Stats(Address),
    /// Set of traders that have ever traded under a given referrer's code,
    /// used to deduplicate the `total_traders` counter.
    SeenTrader(Address, Address),
}

#[contractimpl]
impl ReferralStorage {
    pub fn initialize(env: Env, role_store: Address) {
        if env.storage().instance().has(&ReferralStorageKey::RoleStore) {
            panic!("already initialised");
        }
        env.storage()
            .instance()
            .set(&ReferralStorageKey::RoleStore, &role_store);
    }

    /// Register `referral_code` for `caller` as the code owner.
    pub fn register_code(env: Env, caller: Address, referral_code: BytesN<32>) {
        caller.require_auth();
        if referral_code.to_array() == [0u8; 32] {
            panic!("invalid referral code");
        }
        if env
            .storage()
            .persistent()
            .has(&ReferralStorageKey::CodeOwner(referral_code.clone()))
        {
            panic_with_error!(&env, ReferralError::CodeAlreadyRegistered);
        }
        env.storage()
            .persistent()
            .set(&ReferralStorageKey::CodeOwner(referral_code), &caller);
    }

    /// Set the rebate/discount tier for a referrer. Admin only.
    pub fn set_tier(env: Env, caller: Address, referrer: Address, tier: TierConfig) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage()
            .persistent()
            .set(&ReferralStorageKey::Tier(referrer), &tier);
    }

    pub fn get_code_owner(env: Env, referral_code: BytesN<32>) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&ReferralStorageKey::CodeOwner(referral_code))
    }

    pub fn get_tier(env: Env, referrer: Address) -> TierConfig {
        env.storage()
            .persistent()
            .get(&ReferralStorageKey::Tier(referrer))
            .unwrap_or(TierConfig {
                rebate_bps: 0,
                discount_bps: 0,
            })
    }

    // -----------------------------------------------------------------------
    // Referrer stats (#69)
    // -----------------------------------------------------------------------

    /// Record a referred trade for `referrer`. Increments cumulative volume
    /// and rebates, and the unique-trader counter the first time `trader` is
    /// seen under this referrer.
    ///
    /// Admin-gated: only the trusted increment path (the order pipeline)
    /// should be allowed to write here, so untrusted callers can't inflate
    /// referrer rankings. The order pipeline's caller (or a keeper acting on
    /// its behalf) must hold ROLE_ADMIN.
    pub fn record_referred_trade(
        env: Env,
        caller: Address,
        referrer: Address,
        trader: Address,
        volume_usd: u128,
        rebate: u128,
    ) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let mut stats: ReferrerStats = env
            .storage()
            .persistent()
            .get(&ReferralStorageKey::Stats(referrer.clone()))
            .unwrap_or_default();

        stats.total_referred_volume_usd =
            stats.total_referred_volume_usd.saturating_add(volume_usd);
        stats.total_rebates_earned = stats.total_rebates_earned.saturating_add(rebate);

        // Distinct trader: only bump the counter the first time we see this
        // (referrer, trader) pair.
        let seen_key = ReferralStorageKey::SeenTrader(referrer.clone(), trader);
        if !env.storage().persistent().has(&seen_key) {
            env.storage().persistent().set(&seen_key, &true);
            stats.total_traders_referred = stats.total_traders_referred.saturating_add(1);
        }

        env.storage()
            .persistent()
            .set(&ReferralStorageKey::Stats(referrer), &stats);
    }

    /// Read the cumulative stats for a referrer. Returns the zero-stats
    /// struct if the referrer has never been recorded.
    pub fn get_referrer_stats(env: Env, referrer: Address) -> ReferrerStats {
        env.storage()
            .persistent()
            .get(&ReferralStorageKey::Stats(referrer))
            .unwrap_or_default()
    }

    fn require_admin(env: &Env, caller: &Address) {
        let rs_addr: Address = env
            .storage()
            .instance()
            .get(&ReferralStorageKey::RoleStore)
            .expect("not initialised");
        let rs = RoleStoreClient::new(env, &rs_addr);
        if !rs.has_role(&role_admin_id(env), caller) {
            panic_with_error!(env, ReferralError::Unauthorized);
        }
    }
}
