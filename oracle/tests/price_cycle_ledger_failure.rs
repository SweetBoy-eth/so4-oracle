/// Integration tests for issue #396: when `getLatestLedger` returns an error,
/// the oracle records the failure and aborts the current price cycle without
/// processing any tokens.
///
/// Relevant code: `oracle/src/price_loop.rs::run_price_cycle`
///
/// ```text
/// Err(error) => {
///     record_error(&state, "get_latest_ledger", error.to_string()).await;
///     finish_cycle(&state, started, tokens_ok, tokens_failed).await;
///     return;
/// }
/// ```
///
/// Covered invariants:
/// - Error recorded in `state.failures` with operation = "get_latest_ledger".
/// - Price cache untouched (no tokens processed, `last_updated` stays `None`).
/// - `cycle_status.price_cycle_running` reset to `false` by `finish_cycle`.
/// - `cycle_status.last_price_cycle_at` set even on abort.
/// - Metrics counter incremented (finish_cycle always runs).
/// - A subsequent good cycle succeeds normally after a prior ledger failure.
use std::sync::Arc;
use std::time::Duration;

use shared_config::TokenConfig;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use oracle::config::{Config, Network, PriceFeedConfig, SecretString};
use oracle::price_loop::run_price_cycle;
use oracle::state::AppState;

const USDC_ADDR: &str = "CBAN5YU3KRDKPTQ2H76D6S7HQFPRBGUD524F65BUM2RQCITPTRLKWKES";

fn ledger_ok() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": { "id": "abc", "sequence": 12345, "protocolVersion": "22" }
    })
}

fn ledger_fail() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "error": { "code": -32000, "message": "node unavailable" }
    })
}

fn fixed_token(symbol: &str, address: &str) -> TokenConfig {
    TokenConfig {
        symbol: symbol.to_string(),
        display_symbol: Some(symbol.to_string()),
        stellar_address: address.to_string(),
        sources: vec!["fixed".to_string()],
        fixed_price: Some("1000000000000000000000000000000".to_string()),
        binance_symbol: None,
        coinbase_symbol: None,
        pyth_feed_id: None,
        min_sources: 1,
        max_deviation_bps: 100,
        stale_after_seconds: 60,
        submit_threshold_bps: 10,
        min: 0.0,
        max: 0.0,
        sources_used: vec![],
    }
}

fn test_state(rpc_url: &str, tokens: Vec<TokenConfig>) -> Arc<AppState> {
    let config = Arc::new(Config {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        network: Network::Testnet,
        network_passphrase: "Test SDF Network ; September 2015".to_string(),
        stellar_rpc_url: rpc_url.to_string(),
        horizon_url: "http://localhost:0".to_string(),
        oracle_contract_id: "CORACLE".to_string(),
        role_store_contract_id: "CROLE".to_string(),
        data_store_contract_id: "CDATA".to_string(),
        order_handler_contract_id: "CORDER".to_string(),
        deposit_handler_contract_id: "CDEPOSIT".to_string(),
        withdrawal_handler_contract_id: "CWITHDRAW".to_string(),
        reader_contract_id: "CREADER".to_string(),
        keeper_private_key: SecretString::new(
            "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        ),
        keeper_secret_key: SecretString::new("SSECRET".to_string()),
        keeper_account_id: "GACCOUNT".to_string(),
        keeper_index: 0,
        admin_api_token: None,
        min_keeper_balance_xlm: 0.0,
        price_loop_interval: Duration::from_millis(1000),
        keeper_loop_interval: Duration::from_millis(1000),
        price_feed: PriceFeedConfig { tokens },
    });
    Arc::new(AppState::new(config))
}

#[tokio::test]
async fn ledger_failure_records_error_in_failures() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ledger_fail()))
        .mount(&mock)
        .await;

    let state = test_state(&mock.uri(), vec![fixed_token("USDC", USDC_ADDR)]);

    run_price_cycle(Arc::clone(&state)).await;

    let failures = state.failures.lock().await;
    let entries: Vec<_> = failures.iter().collect();
    assert!(!entries.is_empty(), "an error must be recorded when ledger fetch fails");
    let entry = &entries[0];
    assert_eq!(
        entry.operation, "get_latest_ledger",
        "operation field must be 'get_latest_ledger'"
    );
}

#[tokio::test]
async fn ledger_failure_leaves_price_cache_empty() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ledger_fail()))
        .mount(&mock)
        .await;

    let state = test_state(&mock.uri(), vec![fixed_token("USDC", USDC_ADDR)]);

    run_price_cycle(Arc::clone(&state)).await;

    let cache = state.price_cache.read().await;
    assert!(
        cache.prices.is_empty(),
        "price cache must be empty when cycle aborts at ledger fetch"
    );
}

#[tokio::test]
async fn ledger_failure_leaves_last_updated_none() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ledger_fail()))
        .mount(&mock)
        .await;

    let state = test_state(&mock.uri(), vec![fixed_token("USDC", USDC_ADDR)]);

    run_price_cycle(Arc::clone(&state)).await;

    let cache = state.price_cache.read().await;
    assert!(
        cache.last_updated.is_none(),
        "last_updated must not be set when cycle aborts before processing any token"
    );
}

#[tokio::test]
async fn ledger_failure_resets_cycle_running() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ledger_fail()))
        .mount(&mock)
        .await;

    let state = test_state(&mock.uri(), vec![fixed_token("USDC", USDC_ADDR)]);

    run_price_cycle(Arc::clone(&state)).await;

    let status = state.cycle_status.read().await;
    assert!(
        !status.price_cycle_running,
        "price_cycle_running must be reset by finish_cycle even after ledger failure"
    );
}

#[tokio::test]
async fn ledger_failure_sets_last_price_cycle_at() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ledger_fail()))
        .mount(&mock)
        .await;

    let state = test_state(&mock.uri(), vec![fixed_token("USDC", USDC_ADDR)]);

    run_price_cycle(Arc::clone(&state)).await;

    let status = state.cycle_status.read().await;
    assert!(
        status.last_price_cycle_at.is_some(),
        "last_price_cycle_at must be set by finish_cycle even on abort"
    );
}

#[tokio::test]
async fn ledger_failure_increments_metrics_counter() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ledger_fail()))
        .mount(&mock)
        .await;

    let state = test_state(&mock.uri(), vec![fixed_token("USDC", USDC_ADDR)]);

    run_price_cycle(Arc::clone(&state)).await;

    let metrics = state.metrics.to_response();
    assert_eq!(
        metrics.price_cycle_count, 1,
        "metrics counter must increment even when cycle aborts due to ledger failure"
    );
}

#[tokio::test]
async fn good_cycle_after_ledger_failure_succeeds() {
    let mock_fail = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ledger_fail()))
        .mount(&mock_fail)
        .await;

    let state = test_state(&mock_fail.uri(), vec![fixed_token("USDC", USDC_ADDR)]);
    run_price_cycle(Arc::clone(&state)).await;

    // Switch to a succeeding RPC — simulate next tick recovering.
    let mock_ok = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ledger_ok()))
        .mount(&mock_ok)
        .await;

    let state2 = test_state(&mock_ok.uri(), vec![fixed_token("USDC", USDC_ADDR)]);
    run_price_cycle(Arc::clone(&state2)).await;

    let cache = state2.price_cache.read().await;
    assert!(
        cache.last_updated.is_some(),
        "a subsequent good cycle must update last_updated after a prior ledger failure"
    );
}

#[tokio::test]
async fn two_ledger_failures_each_recorded_separately() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ledger_fail()))
        .mount(&mock)
        .await;

    let state = test_state(&mock.uri(), vec![fixed_token("USDC", USDC_ADDR)]);

    run_price_cycle(Arc::clone(&state)).await;
    run_price_cycle(Arc::clone(&state)).await;

    let failures = state.failures.lock().await;
    let entries: Vec<_> = failures.iter().collect();
    assert_eq!(
        entries.len(),
        2,
        "each failed cycle must produce a separate error record"
    );
    for entry in &entries {
        assert_eq!(entry.operation, "get_latest_ledger");
    }
}

#[tokio::test]
async fn ledger_failure_error_message_is_non_empty() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ledger_fail()))
        .mount(&mock)
        .await;

    let state = test_state(&mock.uri(), vec![fixed_token("USDC", USDC_ADDR)]);

    run_price_cycle(Arc::clone(&state)).await;

    let failures = state.failures.lock().await;
    let entries: Vec<_> = failures.iter().collect();
    assert!(!entries[0].error.is_empty(), "error message must be non-empty");
}

#[tokio::test]
async fn ledger_failure_metrics_count_matches_cycle_count() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ledger_fail()))
        .mount(&mock)
        .await;

    let state = test_state(&mock.uri(), vec![fixed_token("USDC", USDC_ADDR)]);

    for _ in 0..3 {
        run_price_cycle(Arc::clone(&state)).await;
    }

    let metrics = state.metrics.to_response();
    assert_eq!(
        metrics.price_cycle_count, 3,
        "metrics counter must match the number of cycle invocations"
    );
}
