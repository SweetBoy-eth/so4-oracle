use serde::{Deserialize, Serialize};
use worker::{Fetch, Headers, Method, Request, RequestInit};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RpcError {
    NetworkError(String),
    HttpError(u16),
    JsonError(String),
    RpcFault { code: i64, message: String },
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RpcError::NetworkError(msg) => write!(f, "network error: {msg}"),
            RpcError::HttpError(code) => write!(f, "HTTP {code}"),
            RpcError::JsonError(msg) => write!(f, "JSON parse error: {msg}"),
            RpcError::RpcFault { code, message } => {
                write!(f, "RPC fault {code}: {message}")
            }
        }
    }
}

// ── JSON-RPC wire types ──────────────────────────────────────────────────────

#[derive(Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'a str,
    id: u32,
    method: &'a str,
    params: serde_json::Value,
}

#[derive(Deserialize)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<JsonRpcFault>,
}

#[derive(Deserialize)]
struct JsonRpcFault {
    code: i64,
    message: String,
}

// ── getLatestLedger ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct GetLatestLedgerResult {
    sequence: u32,
    id: String,
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
}

/// Parse the raw JSON body returned by a `getLatestLedger` RPC call.
///
/// Kept separate from the HTTP layer so it can be unit-tested without
/// mocking the network.
pub fn parse_latest_ledger_response(body: &str) -> Result<u32, RpcError> {
    let resp: JsonRpcResponse<GetLatestLedgerResult> =
        serde_json::from_str(body).map_err(|e| RpcError::JsonError(e.to_string()))?;

    if let Some(fault) = resp.error {
        return Err(RpcError::RpcFault {
            code: fault.code,
            message: fault.message,
        });
    }

    resp.result
        .ok_or_else(|| RpcError::JsonError("missing 'result' field".to_string()))
        .map(|r| r.sequence)
}

/// Call `getLatestLedger` on the Stellar RPC endpoint and return the current
/// ledger sequence number.
///
/// **Caching note:** call this once per price-update cycle and pass the
/// returned value to any downstream function that needs `ledger_seq`.  This
/// avoids redundant round-trips within a single scheduled invocation.
pub async fn get_latest_ledger_sequence(rpc_url: &str) -> Result<u32, RpcError> {
    let payload = serde_json::to_string(&JsonRpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "getLatestLedger",
        params: serde_json::Value::Array(vec![]),
    })
    .map_err(|e| RpcError::JsonError(e.to_string()))?;

    let body = rpc_post(rpc_url, payload).await?;
    parse_latest_ledger_response(&body)
}

/// Low-level helper: POST a JSON string to the RPC URL, return the response body.
pub(crate) async fn rpc_post(rpc_url: &str, payload: String) -> Result<String, RpcError> {
    let mut headers = Headers::new();
    headers
        .set("Content-Type", "application/json")
        .map_err(|e| RpcError::NetworkError(e.to_string()))?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(payload.into()));

    let request = Request::new_with_init(rpc_url, &init)
        .map_err(|e| RpcError::NetworkError(e.to_string()))?;

    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|e| RpcError::NetworkError(e.to_string()))?;

    let status = response.status_code();
    let body = response
        .text()
        .await
        .map_err(|e| RpcError::NetworkError(e.to_string()))?;

    if status != 200 {
        return Err(RpcError::HttpError(status));
    }

    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_latest_ledger_response() {
        let body = r#"{
            "jsonrpc":"2.0","id":1,
            "result":{"id":"abc123","sequence":12345,"protocolVersion":"22"}
        }"#;
        assert_eq!(parse_latest_ledger_response(body).unwrap(), 12345u32);
    }

    #[test]
    fn parse_rpc_fault_response() {
        let body = r#"{
            "jsonrpc":"2.0","id":1,
            "error":{"code":-32000,"message":"start height out of range"}
        }"#;
        let err = parse_latest_ledger_response(body).unwrap_err();
        assert_eq!(
            err,
            RpcError::RpcFault {
                code: -32000,
                message: "start height out of range".to_string(),
            }
        );
    }

    #[test]
    fn parse_malformed_json_returns_error() {
        let err = parse_latest_ledger_response("not json").unwrap_err();
        assert!(matches!(err, RpcError::JsonError(_)));
    }

    #[test]
    fn parse_missing_result_field() {
        let body = r#"{"jsonrpc":"2.0","id":1}"#;
        let err = parse_latest_ledger_response(body).unwrap_err();
        assert!(matches!(err, RpcError::JsonError(_)));
    }
}
