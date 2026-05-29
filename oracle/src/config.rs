use serde::Deserialize;

pub const ENV_KEY: &str = "PRICE_FEED_CONFIG";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TokenFeedConfig {
    pub symbol: String,
    pub stellar_address: String,
    pub sources: Vec<String>,
    #[serde(default)]
    pub binance_symbol: Option<String>,
    #[serde(default)]
    pub pyth_feed_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PriceFeedConfig {
    pub tokens: Vec<TokenFeedConfig>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    MissingEnvVar,
    MalformedJson(String),
    EmptyTokenList,
    InvalidToken { symbol: String, reason: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingEnvVar => {
                write!(f, "required env var '{ENV_KEY}' is not set")
            }
            ConfigError::MalformedJson(msg) => {
                write!(f, "PRICE_FEED_CONFIG is not valid JSON: {msg}")
            }
            ConfigError::EmptyTokenList => {
                write!(f, "PRICE_FEED_CONFIG must contain at least one token")
            }
            ConfigError::InvalidToken { symbol, reason } => {
                write!(f, "invalid token config for '{symbol}': {reason}")
            }
        }
    }
}

/// Parse and validate the `PRICE_FEED_CONFIG` JSON string.
///
/// Expected format:
/// ```json
/// [{"symbol":"BTC","stellar_address":"C...","sources":["binance","coinbase"]}]
/// ```
pub fn parse_price_feed_config(raw: &str) -> Result<PriceFeedConfig, ConfigError> {
    let tokens: Vec<TokenFeedConfig> =
        serde_json::from_str(raw).map_err(|e| ConfigError::MalformedJson(e.to_string()))?;

    if tokens.is_empty() {
        return Err(ConfigError::EmptyTokenList);
    }

    for token in &tokens {
        if token.symbol.is_empty() {
            return Err(ConfigError::InvalidToken {
                symbol: "(empty)".to_string(),
                reason: "symbol must not be empty".to_string(),
            });
        }
        if token.stellar_address.is_empty() {
            return Err(ConfigError::InvalidToken {
                symbol: token.symbol.clone(),
                reason: "stellar_address must not be empty".to_string(),
            });
        }
        if token.sources.is_empty() {
            return Err(ConfigError::InvalidToken {
                symbol: token.symbol.clone(),
                reason: "sources list must not be empty".to_string(),
            });
        }
        for source in &token.sources {
            if source.is_empty() {
                return Err(ConfigError::InvalidToken {
                    symbol: token.symbol.clone(),
                    reason: "source names must not be empty strings".to_string(),
                });
            }
        }
    }

    Ok(PriceFeedConfig { tokens })
}

/// Load and validate `PRICE_FEED_CONFIG` from the Worker environment.
pub fn load_from_env(env: &worker::Env) -> Result<PriceFeedConfig, ConfigError> {
    let raw = env
        .var(ENV_KEY)
        .map_err(|_| ConfigError::MissingEnvVar)?
        .to_string();
    parse_price_feed_config(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_JSON: &str = r#"[
        {"symbol":"BTC","stellar_address":"CBTCADDR","sources":["binance","coinbase"]},
        {"symbol":"ETH","stellar_address":"CETHADDR","sources":["binance"]}
    ]"#;

    #[test]
    fn parse_valid_config() {
        let cfg = parse_price_feed_config(VALID_JSON).unwrap();
        assert_eq!(cfg.tokens.len(), 2);
        assert_eq!(cfg.tokens[0].symbol, "BTC");
        assert_eq!(cfg.tokens[0].sources, vec!["binance", "coinbase"]);
        assert_eq!(cfg.tokens[1].symbol, "ETH");
        assert_eq!(cfg.tokens[1].sources, vec!["binance"]);
    }

    #[test]
    fn reject_malformed_json() {
        let err = parse_price_feed_config("{not json}").unwrap_err();
        assert!(matches!(err, ConfigError::MalformedJson(_)));
    }

    #[test]
    fn reject_empty_token_list() {
        let err = parse_price_feed_config("[]").unwrap_err();
        assert_eq!(err, ConfigError::EmptyTokenList);
    }

    #[test]
    fn reject_token_with_empty_symbol() {
        let json = r#"[{"symbol":"","stellar_address":"CADDR","sources":["binance"]}]"#;
        let err = parse_price_feed_config(json).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidToken { .. }));
    }

    #[test]
    fn reject_token_with_empty_stellar_address() {
        let json = r#"[{"symbol":"BTC","stellar_address":"","sources":["binance"]}]"#;
        let err = parse_price_feed_config(json).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidToken { ref symbol, .. } if symbol == "BTC"
        ));
    }

    #[test]
    fn reject_token_with_empty_sources() {
        let json = r#"[{"symbol":"BTC","stellar_address":"CADDR","sources":[]}]"#;
        let err = parse_price_feed_config(json).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidToken { ref symbol, .. } if symbol == "BTC"
        ));
    }

    #[test]
    fn per_token_source_list_preserved() {
        let json = r#"[
            {"symbol":"BTC","stellar_address":"CBADDR","sources":["binance"]},
            {"symbol":"ETH","stellar_address":"CEADDR","sources":["coinbase"]}
        ]"#;
        let cfg = parse_price_feed_config(json).unwrap();
        assert_eq!(cfg.tokens[0].sources, vec!["binance"]);
        assert_eq!(cfg.tokens[1].sources, vec!["coinbase"]);
    }
}
