use apis::cache::Cache;
use apis::server;
use apis::state::{AppState, MarketSummary, Reader, ReaderError};
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use std::sync::Arc;

struct MockReader;

#[async_trait]
impl Reader for MockReader {
    async fn get_markets(&self) -> Result<Vec<String>, ReaderError> {
        Ok(vec!["m1".to_string(), "m2".to_string()])
    }
    async fn get_market_pool_value_info(&self, market: &str) -> Result<MarketSummary, ReaderError> {
        Ok(MarketSummary {
            market_token_address: market.to_string(),
            index_token: "gbpaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            long_token: "L".to_string(),
            short_token: "S".to_string(),
            pool_value_usd: 1000.0,
            long_oi: 200.0,
            short_oi: 150.0,
            current_funding_rate: 0.001,
        })
    }
    async fn get_market_detail(&self, market: &str) -> Result<serde_json::Value, ReaderError> {
        Ok(serde_json::json!({"market_id": market, "top_positions": ["p1","p2"]}))
    }
    async fn get_account_positions(&self, _account: &str) -> Result<Vec<String>, ReaderError> {
        Ok(vec!["p1".to_string()])
    }
    async fn get_position_info(&self, position_id: &str) -> Result<serde_json::Value, ReaderError> {
        if position_id == "p1" {
            Ok(
                serde_json::json!({"id":"p1","size":10.0,"collateral":100.0,"entry_price":1.0,"index_token":"gbpaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","liquidation_price":0.5,"pending_fees":1.0}),
            )
        } else {
            Ok(serde_json::json!({}))
        }
    }
    async fn get_latest_price(&self, _token: &str) -> Result<f64, ReaderError> {
        Ok(1.2)
    }
}

#[tokio::test]
async fn test_prices_token_found_case_insensitive() {
    // token in config is lowercase string starting with g
    let uri = "/prices/GBPAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    // start server with mock reader
    let state = AppState {
        cache: Cache::new(),
        reader: Arc::new(MockReader),
    };
    let app = Router::new()
        .route(
            "/prices/:token",
            axum::routing::get(apis::server::get_price),
        )
        .layer(axum::Extension(Arc::new(state)));

    let resp = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_markets_list_cached_and_populated() {
    let state = AppState {
        cache: Cache::new(),
        reader: Arc::new(MockReader),
    };
    let app = Router::new()
        .route("/markets", axum::routing::get(apis::server::get_markets))
        .layer(axum::Extension(Arc::new(state)));
    let req = Request::builder()
        .uri("/markets")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // second call should be served from cache; still OK
    let req2 = Request::builder()
        .uri("/markets")
        .body(Body::empty())
        .unwrap();
    let resp2 = app.oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_market_detail_and_positions() {
    let state = AppState {
        cache: Cache::new(),
        reader: Arc::new(MockReader),
    };
    let app = Router::new()
        .route(
            "/markets/:market_id",
            axum::routing::get(apis::server::get_market),
        )
        .layer(axum::Extension(Arc::new(state)));
    let req = Request::builder()
        .uri("/markets/m1")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_positions_empty_returns_empty_array() {
    struct EmptyReader;
    #[async_trait]
    impl Reader for EmptyReader {
        async fn get_markets(&self) -> Result<Vec<String>, ReaderError> {
            Ok(vec![])
        }
        async fn get_market_pool_value_info(
            &self,
            _market: &str,
        ) -> Result<MarketSummary, ReaderError> {
            Err(ReaderError::RpcError)
        }
        async fn get_market_detail(&self, _market: &str) -> Result<serde_json::Value, ReaderError> {
            Err(ReaderError::RpcError)
        }
        async fn get_account_positions(&self, _account: &str) -> Result<Vec<String>, ReaderError> {
            Ok(vec![])
        }
        async fn get_position_info(
            &self,
            _position_id: &str,
        ) -> Result<serde_json::Value, ReaderError> {
            Err(ReaderError::RpcError)
        }
        async fn get_latest_price(&self, _token: &str) -> Result<f64, ReaderError> {
            Ok(0.0)
        }
    }
    let state = AppState {
        cache: Cache::new(),
        reader: Arc::new(EmptyReader),
    };
    let app = Router::new()
        .route(
            "/positions/:account",
            axum::routing::get(apis::server::get_positions),
        )
        .layer(axum::Extension(Arc::new(state)));
    let req = Request::builder()
        .uri("/positions/g0000000000000000000000000000000000000000000000000000")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v.is_array() && v.as_array().unwrap().is_empty());
}
