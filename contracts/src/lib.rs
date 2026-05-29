#![no_std]
#![allow(
    mismatched_lifetime_syntaxes,
    clippy::too_many_arguments,
    clippy::cast_lossless,
    clippy::needless_borrow,
    clippy::doc_overindented_list_items,
    clippy::unnecessary_cast,
    clippy::needless_return
)]

pub mod adl_handler;
pub mod config_handler;
pub mod data_store;
pub mod decrease_position_utils;
pub mod deposit_vault;
pub mod fee_handler;
pub mod increase_position_utils;
pub mod insurance_fund;
pub mod keys;
pub mod libs;
pub mod liquidity_handler;
pub mod market_factory;
pub mod market_token;
pub mod market_utils;
pub mod order_handler;
pub mod position_handler;
pub mod position_utils;
pub mod pricing_utils;
pub mod reader;
pub mod referral_storage;
pub mod referral_utils;
pub mod role_store;
pub mod router;
pub mod swap_utils;
pub mod types;
