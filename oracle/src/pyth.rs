use serde::Deserialize;
use worker::{Fetch, Url};

pub const PYTH_HERMES_URL: &str = "https://hermes.pyth.network/api/latest_price_feeds";
pub const FLOAT_PRECISION: i128 = 1_000_000_000_000_000_000_000_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythPriceError {
    NetworkError(String),
    HttpError(u16),
    JsonError(String),
    PriceParseError(String),
    MissingFeedId(String),
}

#[derive(Debug, Deserialize)]
pub struct PythPrice {
    pub price: PythPriceData,
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct PythPriceData {
    pub price: String,
    pub expo: i32,
}

#[derive(Debug, Deserialize)]
pub struct PythPriceFeed {
    pub id: String,
    pub price: PythPriceData,
}

#[derive(Debug, Deserialize)]
pub struct PythResponse {
    pub data: PythPriceFeed,
}

pub fn normalize_pyth_price(price_str: &str, exponent: i32) -> Result<i128, PythPriceError> {
    let price_int = price_str
        .trim()
        .parse::<i128>()
        .map_err(|_| PythPriceError::PriceParseError(format!("invalid price: {}", price_str)))?;

    if price_int < 0 {
        return Err(PythPriceError::PriceParseError(
            "negative prices not supported".to_string(),
        ));
    }

    let target_exponent: i32 = -30;
    let exponent_diff = target_exponent - exponent;

    if exponent_diff >= 0 {
        price_int
            .checked_mul(10i128.pow(exponent_diff as u32))
            .ok_or_else(|| {
                PythPriceError::PriceParseError("price overflow during normalization".to_string())
            })
    } else {
        let divisor = 10i128.pow((-exponent_diff) as u32);
        Ok(price_int / divisor)
    }
}

pub async fn fetch_pyth_price(feed_id: &str) -> Result<i128, PythPriceError> {
    let url_string = format!("{}?ids[]={}", PYTH_HERMES_URL, feed_id);
    let url = Url::parse(&url_string)
        .map_err(|err| PythPriceError::NetworkError(err.to_string()))?;

    let mut response = Fetch::Url(url)
        .send()
        .await
        .map_err(|err| PythPriceError::NetworkError(err.to_string()))?;

    let status = response.status_code();
    if status != 200 {
        return Err(PythPriceError::HttpError(status));
    }

    let body = response
        .text()
        .await
        .map_err(|err| PythPriceError::NetworkError(err.to_string()))?;

    let feed: PythResponse =
        serde_json::from_str(&body).map_err(|err| PythPriceError::JsonError(err.to_string()))?;

    normalize_pyth_price(&feed.data.price.price, feed.data.price.expo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_pyth_price_positive_exponent() {
        let result = normalize_pyth_price("45000000000", 8).unwrap();
        let expected = 45000000000i128 / 10i128.pow(38);
        assert_eq!(result, expected);
    }

    #[test]
    fn normalize_pyth_price_negative_exponent() {
        let result = normalize_pyth_price("4500000000", -8).unwrap();
        assert!(result > 0);
    }

    #[test]
    fn normalize_pyth_price_invalid() {
        let err = normalize_pyth_price("invalid", -8).unwrap_err();
        assert!(matches!(err, PythPriceError::PriceParseError(_)));
    }

    #[test]
    fn normalize_pyth_price_negative() {
        let err = normalize_pyth_price("-45000000000", -8).unwrap_err();
        assert!(matches!(err, PythPriceError::PriceParseError(_)));
    }
}
