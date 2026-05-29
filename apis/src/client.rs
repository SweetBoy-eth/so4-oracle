use crate::state::{MarketSummary, Reader, ReaderError};
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;

async fn retry<T, F>(mut f: F) -> Result<T, ReaderError>
where
    F: FnMut() -> futures::future::BoxFuture<'static, Result<T, ReaderError>>,
{
    let mut backoff = 50u64;
    for _ in 0..3 {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                sleep(Duration::from_millis(backoff)).await;
                backoff *= 2;
                if backoff > 400 {
                    return Err(e);
                }
            }
        }
    }
    f().await
}

pub struct RpcClient;

#[async_trait]
impl Reader for RpcClient {
    async fn get_markets(&self) -> Result<Vec<String>, ReaderError> {
        retry(|| Box::pin(async { Ok(Vec::<String>::new()) })).await
    }

    async fn get_market_pool_value_info(
        &self,
        _market: &str,
    ) -> Result<MarketSummary, ReaderError> {
        retry(|| {
            Box::pin(async {
                Ok(MarketSummary {
                    market_token_address: String::new(),
                    index_token: String::new(),
                    long_token: String::new(),
                    short_token: String::new(),
                    pool_value_usd: 0.0,
                    long_oi: 0.0,
                    short_oi: 0.0,
                    current_funding_rate: 0.0,
                })
            })
        })
        .await
    }

    async fn get_market_detail(&self, _market: &str) -> Result<serde_json::Value, ReaderError> {
        retry(|| Box::pin(async { Ok(json!({})) })).await
    }

    async fn get_account_positions(&self, _account: &str) -> Result<Vec<String>, ReaderError> {
        retry(|| Box::pin(async { Ok(Vec::<String>::new()) })).await
    }

    async fn get_position_info(
        &self,
        _position_id: &str,
    ) -> Result<serde_json::Value, ReaderError> {
        retry(|| Box::pin(async { Ok(json!({})) })).await
    }

    async fn get_latest_price(&self, _token: &str) -> Result<f64, ReaderError> {
        retry(|| Box::pin(async { Ok(0.0) })).await
    }
}
