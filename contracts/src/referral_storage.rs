use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, Address, BytesN, Env};

use crate::{
    role_store::{role_admin_id, RoleStoreClient},
    types::{ReferralError, TierConfig},
};

#[contract]
pub struct ReferralStorage;

#[contracttype]
enum ReferralStorageKey {
    RoleStore,
    CodeOwner(BytesN<32>),
    Tier(Address),
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
