use crate::cache::Cache;
use crate::config::lookup_token;
use crate::history::{HistoryStore, Interval};
use crate::state::{AppState, MarketSummary, Reader, ReaderError};
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Extension, Json, Router,
};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

pub async fn run() -> Result<(), anyhow::Error> {
    let cache = Cache::new();
    let reader = Arc::new(crate::client::RpcClient) as Arc<dyn Reader + Send + Sync>;
    let history = HistoryStore::new();

    // Background task: record a price tick every 60 seconds for all known tokens.
    let history_bg = history.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let ts = chrono::Utc::now().timestamp() as u64;
            // Iterate over all tokens in the static config and record mid-price.
            if let Some(tokens) = crate::config::all_tokens() {
                for entry in tokens {
                    let mid = (entry.min + entry.max) / 2.0;
                    history_bg.record(&entry.token, ts, mid);
                }
            }
        }
    });

    let state = AppState { cache, reader, history };

    let app = Router::new()
        .route("/health", get(health))
        .route("/prices/:token", get(get_price))
        .route("/prices/:token/history", get(get_price_history))
        .route("/markets", get(get_markets))
        .route("/markets/:market_id", get(get_market))
        .route("/positions/:account", get(get_positions))
        .layer(Extension(Arc::new(state)));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    println!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status":"ok"}))
}

// ── GET /prices/:token/history ───────────────────────────────────────────────

#[derive(Deserialize)]
struct HistoryQuery {
    /// Candle interval: "1m", "5m", or "1h" (default: "1m").
    interval: Option<String>,
    /// Unix timestamp (seconds) — start of range (default: now − 24 h).
    from: Option<u64>,
    /// Unix timestamp (seconds) — end of range (default: now).
    to: Option<u64>,
}

pub async fn get_price_history(
    Path(token): Path<String>,
    Query(params): Query<HistoryQuery>,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    let interval_str = params.interval.as_deref().unwrap_or("1m");
    let interval = match Interval::from_str(interval_str) {
        Some(i) => i,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid interval — use 1m, 5m, or 1h"})),
            )
                .into_response();
        }
    };

    let now = chrono::Utc::now().timestamp() as u64;
    let to = params.to.unwrap_or(now);
    let from = params.from.unwrap_or_else(|| to.saturating_sub(86_400));

    if from > to {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "`from` must be <= `to`"})),
        )
            .into_response();
    }

    match state.history.query(&token, from, to, interval) {
        Some(candles) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "token": token.to_lowercase(),
                "interval": interval_str,
                "from": from,
                "to": to,
                "candles": candles,
            })),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "no history for token"})),
        )
            .into_response(),
    }
}

#[derive(Serialize)]
struct PriceResp {
    token: String,
    symbol: String,
    min: f64,
    max: f64,
    timestamp: i64,
    sources_used: Vec<String>,
}

pub async fn get_price(
    Path(token): Path<String>,
    Extension(_state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    let key = token.to_lowercase();
    if let Some(entry) = lookup_token(&key) {
        let resp = PriceResp {
            token: entry.token.clone(),
            symbol: entry.symbol,
            min: entry.min,
            max: entry.max,
            timestamp: chrono::Utc::now().timestamp(),
            sources_used: entry.sources_used.clone(),
        };
        return (StatusCode::OK, Json(resp)).into_response();
    }
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error":"token not found in feed"})),
    )
        .into_response()
}

pub async fn get_markets(Extension(state): Extension<Arc<AppState>>) -> impl IntoResponse {
    // cache key
    let cache_key = "markets_list";
    if let Some(cached) = state.cache.get::<Vec<MarketSummary>>(cache_key).await {
        return (StatusCode::OK, Json(cached));
    }

    let markets = match state.reader.get_markets().await {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error":"rpc failure"})),
            )
        }
    };

    let futs = markets.into_iter().map(|m| {
        let r = state.reader.clone();
        async move { r.get_market_pool_value_info(&m).await }
    });

    let results = join_all(futs).await;
    let mut out = Vec::new();
    for r in results {
        if let Ok(s) = r {
            out.push(s);
        }
    }

    let ttl = Duration::from_secs(30);
    state.cache.set(cache_key, &out, ttl).await;

    (StatusCode::OK, Json(out))
}

async fn get_market(
    Path(market_id): Path<String>,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    let key = format!("market_detail:{}", market_id.to_lowercase());
    if let Some(cached) = state.cache.get::<serde_json::Value>(&key).await {
        return (StatusCode::OK, Json(cached));
    }

    let detail = match state.reader.get_market_detail(&market_id).await {
        Ok(v) => v,
        Err(ReaderError::NotFound) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error":"market not found"})),
            )
        }
        Err(_) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error":"rpc failure"})),
            )
        }
    };

    // For top positions, assume detail contains position ids under "top_positions"
    let top_positions: Vec<String> = detail
        .get("top_positions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let futs = top_positions.iter().map(|p| {
        let r = state.reader.clone();
        let pid = p.clone();
        async move { r.get_position_info(&pid).await }
    });
    let pos_results = join_all(futs).await;
    let mut positions = Vec::new();
    for r in pos_results {
        if let Ok(v) = r {
            positions.push(v);
        }
    }

    let resp = serde_json::json!({
        "market": detail,
        "top_positions": positions,
    });

    let ttl = Duration::from_secs(15);
    state.cache.set(&key, &resp, ttl).await;

    (StatusCode::OK, Json(resp))
}

async fn get_positions(
    Path(account): Path<String>,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    let acct = account.to_lowercase();
    // validate simple format
    if acct.len() != 56 || !(acct.starts_with('g') || acct.starts_with('G')) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid account"})),
        );
    }

    let positions = match state.reader.get_account_positions(&acct).await {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error":"rpc failure"})),
            )
        }
    };

    if positions.is_empty() {
        return (StatusCode::OK, Json(serde_json::json!([])));
    }

    let futs = positions.iter().map(|p| {
        let r = state.reader.clone();
        let pid = p.clone();
        async move { r.get_position_info(&pid).await }
    });
    let pos_results = join_all(futs).await;
    let mut out = Vec::new();
    for r in pos_results {
        if let Ok(mut v) = r {
            // compute pnl using latest price for position's index token
            if let Some(idx) = v.get("index_token").and_then(|x| x.as_str()) {
                if let Ok(price) = state.reader.get_latest_price(idx).await {
                    if let Some(entry_price) = v.get("entry_price").and_then(|x| x.as_f64()) {
                        let size = v.get("size").and_then(|x| x.as_f64()).unwrap_or(0.0);
                        let pnl = (price - entry_price) * size;
                        v.as_object_mut().map(|m| {
                            m.insert("current_pnl".to_string(), serde_json::json!(pnl));
                        });
                    }
                }
            }
            out.push(v);
        }
    }

    (StatusCode::OK, Json(out))
}
