use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum LiquidityError {
    NotInitialized = 10,
    Unauthorized = 11,
    MarketNotRegistered = 12,
    PricesNotSet = 13,
    InsufficientLp = 14,
    InsufficientLongOut = 15,
    InsufficientShortOut = 16,
    WithdrawalNotFound = 17,
    ZeroAmount = 18,
}

impl From<LiquidityError> for soroban_sdk::Error {
    fn from(e: LiquidityError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketTokens {
    pub long_token: Address,
    pub short_token: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OraclePrices {
    pub long_price: u128,
    pub short_price: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Withdrawal {
    pub account: Address,
    pub market_id: u32,
    pub lp_amount: u128,
    pub receiver: Address,
    pub min_long_out: u128,
    pub min_short_out: u128,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MarketError {
    Unauthorized = 1,
    MarketNotFound = 2,
    MarketPaused = 3,
    MarketAlreadyExists = 4,
}

impl From<MarketError> for soroban_sdk::Error {
    fn from(e: MarketError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketConfig {
    pub max_long_open_interest: u128,
    pub max_short_open_interest: u128,
    pub maintenance_margin_factor: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketProps {
    pub market_id: u32,
    pub long_token: Address,
    pub short_token: Address,
    pub market_token: Address,
    pub max_long_open_interest: u128,
    pub max_short_open_interest: u128,
    pub is_paused: bool,
    pub maintenance_margin_factor: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionProps {
    pub position_key: BytesN<32>,
    pub account: Address,
    pub market_id: u32,
    pub quantity: u128,
    pub collateral_amount: u128,
    pub average_price: u128,
    pub is_long: bool,
    pub is_open: bool,
    /// Referral code hash; all-zero means no referral.
    pub referral_code: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionFees {
    pub borrowing_fee: u128,
    pub funding_fee: u128,
    pub position_fee: u128,
    pub total_fee: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundingInfo {
    pub borrowing_factor: u128,
    pub funding_factor: u128,
    pub position_fee_factor: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionInfo {
    pub position: PositionProps,
    pub pnl_usd: i128,
    pub pending_fees: PositionFees,
    pub liquidation_price: u128,
    pub funding_info: FundingInfo,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolValueInfo {
    pub pool_value: u128,
    pub long_pnl: i128,
    pub short_pnl: i128,
    pub impact_pool_amount: u128,
    pub net_pnl: i128,
    pub lp_supply: u128,
    pub index_token_price: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionPriceResult {
    pub price_without_impact: i128,
    pub price_with_impact: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TierConfig {
    /// Basis points of the position fee rebated to the code owner.
    pub rebate_bps: u32,
    /// Basis points of the position fee discounted for the trader.
    pub discount_bps: u32,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ReferralError {
    Unauthorized = 60,
    CodeAlreadyRegistered = 61,
    CodeNotFound = 62,
}

impl From<ReferralError> for soroban_sdk::Error {
    fn from(e: ReferralError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PriceError {
    PriceTooHigh = 70,
    PriceTooLow = 71,
}

impl From<PriceError> for soroban_sdk::Error {
    fn from(e: PriceError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PositionError {
    PositionNotFound = 30,
    MaxOpenInterestExceeded = 20,
    InsufficientCollateral = 21,
}

impl From<PositionError> for soroban_sdk::Error {
    fn from(e: PositionError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouterAction {
    SendTokens(Address, Address, u128),
    CreateDeposit(u32, u128, u128, Address),
    CancelDeposit(u32),
    CreateWithdrawal(u32, u128, Address, u128, u128),
    CancelWithdrawal(u32),
    CreateOrder(u32, i128, bool),
    UpdateOrder(u32, i128),
    CancelOrder(u32),
    ClaimFundingFees(u32, Address),
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OrderError {
    UnsatisfiedTrigger = 40,
    OrderNotFound = 41,
    InsufficientOutput = 42,
    Unauthorized = 43,
    CannotUpdateMarketOrder = 44,
    OrderNotExpired = 45,
    InvalidOrderType = 46,
}

impl From<OrderError> for soroban_sdk::Error {
    fn from(e: OrderError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    pub account: Address,
    pub market_id: u32,
    pub is_long: bool,
    pub size_in_usd: u128,
    pub size_in_tokens: u128,
    pub collateral_amount: u128,
    pub referral_code: BytesN<32>,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum OrderType {
    MarketIncrease,
    MarketDecrease,
    LimitIncrease,
    LimitDecrease,
    StopIncrease,
    StopLossDecrease,
    LimitSwap,
    MarketSwap,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Order {
    pub key: u32,
    pub account: Address,
    pub market_id: u32,
    pub order_type: OrderType,
    pub is_long: bool,
    pub size_delta_usd: u128,
    pub collateral_delta: u128,
    pub trigger_price: u128,
    pub acceptable_price: u128,
    pub min_output_amount: u128,
    pub collateral_token: Address,
    pub amount_in: u128,
    pub created_at: u32,
    pub is_frozen: bool,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ConfigError {
    Unauthorized = 20,
    MarketNotFound = 21,
}

impl From<ConfigError> for soroban_sdk::Error {
    fn from(e: ConfigError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AdlError {
    AdlNotRequired = 50,
    NotProfitable = 51,
    Unauthorized = 52,
    PositionNotFound = 53,
}

impl From<AdlError> for soroban_sdk::Error {
    fn from(e: AdlError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InsuranceFundError {
    Unauthorized = 80,
}

impl From<InsuranceFundError> for soroban_sdk::Error {
    fn from(e: InsuranceFundError) -> Self {
        soroban_sdk::Error::from_contract_error(e as u32)
    }
}

/// Snapshot of a market's funding state for off-chain consumers (#73).
///
/// Read from `funding_factor_key` and the open-interest keys; assembled by
/// `Reader::get_funding_info`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundingInfo {
    /// Signed funding factor per second; positive = longs pay shorts.
    pub funding_factor_per_second: i128,
    /// Current open interest on the long side, in USD (factor-scaled).
    pub open_interest_long: u128,
    /// Current open interest on the short side, in USD (factor-scaled).
    pub open_interest_short: u128,
    /// Total funding fees pending claim on the long side (per-token totals are
    /// keyed by token; this is the protocol's aggregate, or 0 if not tracked
    /// yet — see #67 for per-account claimable funding).
    pub claimable_funding_long: u128,
    /// Total funding fees pending claim on the short side.
    pub claimable_funding_short: u128,
}

/// Snapshot of a market's open interest for off-chain consumers (#74).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenInterestInfo {
    pub long_usd: u128,
    pub short_usd: u128,
    pub max_long_usd: u128,
    pub max_short_usd: u128,
}

/// Cumulative referrer statistics (#69): aggregate volume, rebates earned and
/// distinct traders referred. Updated on each referred trade.
#[contracttype]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReferrerStats {
    pub total_referred_volume_usd: u128,
    pub total_rebates_earned: u128,
    pub total_traders_referred: u32,
}
