use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct TokenFeedEntry {
    pub token: String,
    pub symbol: String,
    pub min: f64,
    pub max: f64,
    pub sources_used: Vec<String>,
}

static TOKENS: Lazy<HashMap<String, TokenFeedEntry>> = Lazy::new(|| {
    let path = "config/tokens.json";
    let raw = fs::read_to_string(path).unwrap_or_else(|_| "{}".to_string());
    let v: Vec<TokenFeedEntry> = serde_json::from_str(&raw).unwrap_or_default();
    let mut m = HashMap::new();
    for mut e in v {
        let key = e.token.to_lowercase();
        e.token = key.clone();
        m.insert(key, e);
    }
    m
});

pub fn lookup_token(addr: &str) -> Option<TokenFeedEntry> {
    TOKENS.get(&addr.to_lowercase()).cloned()
}

/// Return all configured token entries (used by the history background task).
pub fn all_tokens() -> Option<Vec<TokenFeedEntry>> {
    let tokens: Vec<TokenFeedEntry> = TOKENS.values().cloned().collect();
    if tokens.is_empty() { None } else { Some(tokens) }
}
