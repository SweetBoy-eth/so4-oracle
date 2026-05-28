use axum::{routing::get, Router};
use tower_service::Service;
use worker::*;

pub mod binance;
pub mod config;
pub mod stellar_rpc;
pub mod submit;

fn router() -> Router {
    Router::new().route("/", get(root))
}

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    _env: Env,
    _ctx: Context,
) -> Result<axum::http::Response<axum::body::Body>> {
    Ok(router().call(req).await?)
}

/// Scheduled handler — runs the full price-update pipeline on every cron tick.
///
/// Local testing: `wrangler dev --test-scheduled`
/// (triggers a synthetic scheduled event against the local dev server)
#[event(scheduled)]
async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) -> Result<()> {
    // 1. Parse feed configuration from the PRICE_FEED_CONFIG env var.
    let feed_cfg = match config::load_from_env(&env) {
        Ok(cfg) => cfg,
        Err(e) => {
            console_error!("[oracle] startup config error: {e}");
            return Err(Error::from(e.to_string()));
        }
    };

    // 2. Read required env vars.
    let rpc_url = env
        .var("STELLAR_RPC_URL")
        .map_err(|_| Error::from("STELLAR_RPC_URL is not set"))?
        .to_string();

    // 3. Fetch the current ledger sequence once and reuse it for the whole cycle.
    let ledger_seq = match stellar_rpc::get_latest_ledger_sequence(&rpc_url).await {
        Ok(seq) => {
            console_log!("[oracle] ledger sequence: {seq}");
            seq
        }
        Err(e) => {
            console_error!("[oracle] failed to fetch ledger sequence: {e}");
            return Err(Error::from(e.to_string()));
        }
    };

    // 4. Fetch and aggregate prices for every configured token.
    let binance_symbols: Vec<String> = feed_cfg
        .tokens
        .iter()
        .filter(|t| t.sources.iter().any(|s| s == "binance"))
        .map(|t| format!("{}USDT", t.symbol))
        .collect();

    let prices = if !binance_symbols.is_empty() {
        match binance::fetch_spot_prices(&binance_symbols).await {
            Ok(p) => {
                console_log!("[oracle] fetched {} price(s) from Binance", p.len());
                p
            }
            Err(e) => {
                console_error!("[oracle] Binance fetch error: {e:?}");
                return Err(Error::from("price fetch failed"));
            }
        }
    } else {
        vec![]
    };

    if prices.is_empty() {
        console_log!("[oracle] no prices to submit at ledger {ledger_seq}");
        return Ok(());
    }

    // 5. TODO: sign the price vector with the keeper ed25519 key and build the
    //    Soroban `set_prices` transaction XDR (requires KEEPER_SECRET_KEY +
    //    ORACLE_CONTRACT_ID env vars and the soroban-client transaction builder).
    //    Once the signed XDR is ready, submit it via:
    //
    //    let ledger_confirmed = submit::submit_and_poll(&rpc_url, &signed_xdr).await?;
    //    console_log!("[oracle] prices committed at ledger {ledger_confirmed}");

    console_log!(
        "[oracle] scheduled cycle complete — ledger_seq={ledger_seq} prices={:?}",
        prices
    );
    Ok(())
}

pub async fn root() -> &'static str {
    "Hello Axum!"
}
