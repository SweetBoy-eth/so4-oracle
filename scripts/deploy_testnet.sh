#!/usr/bin/env bash
# Deploy SO4 Markets contracts to Stellar testnet in dependency order.
# Idempotent: skips contracts already recorded in the state file.
#
# Prerequisites:
#   - stellar CLI (https://developers.stellar.org/docs/tools/cli)
#   - Funded deployer identity: stellar keys add deployer --network testnet
#   - Built WASM: scripts/build_contracts.sh (or set CONTRACTS_WASM)
#
# Usage:
#   ./scripts/deploy_testnet.sh
#   DEPLOYER=deployer NETWORK=testnet ./scripts/deploy_testnet.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE_FILE="${DEPLOY_STATE_FILE:-$ROOT/scripts/.deploy-testnet-state}"
ENV_FILE="${ENV_OUTPUT:-$ROOT/.env.testnet}"
NETWORK="${NETWORK:-testnet}"
DEPLOYER="${DEPLOYER:-deployer}"
RPC_URL="${STELLAR_RPC_URL:-https://soroban-testnet.stellar.org}"
CONTRACTS_WASM="${CONTRACTS_WASM:-$ROOT/target/wasm32v1-none/release/contracts.wasm}"

log() { printf '[deploy] %s\n' "$*"; }
die() { printf '[deploy] ERROR: %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# State helpers (idempotency)
# ---------------------------------------------------------------------------
state_get() {
  local key="$1"
  if [[ -f "$STATE_FILE" ]]; then
    grep -E "^${key}=" "$STATE_FILE" 2>/dev/null | tail -1 | cut -d= -f2-
  fi
}

state_set() {
  local key="$1" val="$2"
  mkdir -p "$(dirname "$STATE_FILE")"
  touch "$STATE_FILE"
  if grep -qE "^${key}=" "$STATE_FILE" 2>/dev/null; then
    sed -i "s|^${key}=.*|${key}=${val}|" "$STATE_FILE"
  else
    echo "${key}=${val}" >>"$STATE_FILE"
  fi
}

# ---------------------------------------------------------------------------
# Stellar helpers
# ---------------------------------------------------------------------------
stellar_cmd() {
  stellar --network "$NETWORK" --source-account "$DEPLOYER" "$@"
}

admin_addr() {
  stellar keys address "$DEPLOYER" --network "$NETWORK"
}

deploy_contract() {
  local name="$1"
  local existing
  existing="$(state_get "$name")"
  if [[ -n "$existing" ]]; then
    log "skip deploy $name (already $existing)"
    echo "$existing"
    return
  fi

  [[ -f "$CONTRACTS_WASM" ]] || die "WASM not found at $CONTRACTS_WASM — run scripts/build_contracts.sh first"

  log "deploying $name ..."
  local id
  id="$(stellar_cmd contract deploy \
    --wasm "$CONTRACTS_WASM" \
    --alias "so4_${name}" \
    --rpc-url "$RPC_URL" \
    --ignore-checks 2>/dev/null | tail -1)"
  [[ -n "$id" ]] || die "failed to deploy $name"
  state_set "$name" "$id"
  log "$name => $id"
  echo "$id"
}

invoke() {
  local contract="$1" func="$2"
  shift 2
  stellar_cmd contract invoke \
    --id "$contract" \
    --rpc-url "$RPC_URL" \
    -- "$func" "$@" 2>/dev/null || true
}

# ---------------------------------------------------------------------------
# Build WASM if missing
# ---------------------------------------------------------------------------
if [[ ! -f "$CONTRACTS_WASM" ]]; then
  log "WASM missing; running build_contracts.sh"
  "$ROOT/scripts/build_contracts.sh"
fi

ADMIN="$(admin_addr)"
log "deployer=$DEPLOYER admin=$ADMIN network=$NETWORK"

# ---------------------------------------------------------------------------
# 1. Install WASM (market_token — upload only)
# ---------------------------------------------------------------------------
WASM_HASH="$(state_get WASM_HASH)"
if [[ -z "$WASM_HASH" ]]; then
  log "installing WASM ..."
  WASM_HASH="$(stellar_cmd contract upload --wasm "$CONTRACTS_WASM" --rpc-url "$RPC_URL" | tail -1)"
  state_set WASM_HASH "$WASM_HASH"
fi
log "WASM_HASH=$WASM_HASH"

# ---------------------------------------------------------------------------
# 2–17. Deploy contracts in dependency order
# ---------------------------------------------------------------------------
ROLE_STORE="$(deploy_contract role_store)"
DATA_STORE="$(deploy_contract data_store)"

# market_token: wasm hash only (no separate contract id)
MARKET_FACTORY="$(deploy_contract market_factory)"

ORACLE="${ORACLE_CONTRACT_ID:-$(state_get oracle)}"
if [[ -z "$ORACLE" ]]; then
  log "oracle: set ORACLE_CONTRACT_ID for an existing on-chain oracle, or deploy separately"
  ORACLE=""
fi

DEPOSIT_VAULT="$(deploy_contract deposit_vault)"
LIQUIDITY_HANDLER="$(deploy_contract liquidity_handler)"
ORDER_HANDLER="$(deploy_contract order_handler)"
POSITION_HANDLER="$(deploy_contract position_handler)"
ADL_HANDLER="$(deploy_contract adl_handler)"
FEE_HANDLER="$(deploy_contract fee_handler)"
REFERRAL_STORAGE="$(deploy_contract referral_storage)"
READER="$(deploy_contract reader)"
EXCHANGE_ROUTER="$(deploy_contract exchange_router)"

# Logical aliases (issue #111 names → deployed instances)
WITHDRAWAL_VAULT="${LIQUIDITY_HANDLER}"
ORDER_VAULT="${ORDER_HANDLER}"
DEPOSIT_HANDLER="${LIQUIDITY_HANDLER}"
WITHDRAWAL_HANDLER="${LIQUIDITY_HANDLER}"
LIQUIDATION_HANDLER="${POSITION_HANDLER}"

# ---------------------------------------------------------------------------
# Initialisation (skip if already initialised — invoke may no-op on error)
# ---------------------------------------------------------------------------
log "initialising contracts ..."

invoke "$ROLE_STORE" initialize --initial-admin "$ADMIN"
invoke "$DATA_STORE" initialize --admin "$ADMIN"
invoke "$MARKET_FACTORY" initialize --role-store "$ROLE_STORE" --data-store "$DATA_STORE"
invoke "$LIQUIDITY_HANDLER" initialize --role-store "$ROLE_STORE" --data-store "$DATA_STORE"
invoke "$ORDER_HANDLER" initialize --data-store "$DATA_STORE"
invoke "$ORDER_HANDLER" configure --role-store "$ROLE_STORE" --liquidity-handler "$LIQUIDITY_HANDLER"
invoke "$POSITION_HANDLER" initialize --data-store "$DATA_STORE" --liquidity-handler "$LIQUIDITY_HANDLER"
invoke "$ADL_HANDLER" initialize --data-store "$DATA_STORE" --liquidity-handler "$LIQUIDITY_HANDLER"
invoke "$FEE_HANDLER" initialize --data-store "$DATA_STORE" --fee-receiver "$ADMIN"
invoke "$REFERRAL_STORAGE" initialize --role-store "$ROLE_STORE"
invoke "$READER" initialize --data-store "$DATA_STORE" --liquidity-handler "$LIQUIDITY_HANDLER"
invoke "$EXCHANGE_ROUTER" initialize --liquidity-handler "$LIQUIDITY_HANDLER"
invoke "$DEPOSIT_VAULT" initialize --role-store "$ROLE_STORE" --controller "$LIQUIDITY_HANDLER"
invoke "$ORDER_HANDLER" set-adl-handler --caller "$ADMIN" --adl-handler "$ADL_HANDLER"
invoke "$ORDER_HANDLER" set-referral-storage --caller "$ADMIN" --referral-storage "$REFERRAL_STORAGE"

# Grant order keeper to deployer
KEEPER_ROLE="0000000000000000000000004f524445525f4b454550455200000000000000"
invoke "$ROLE_STORE" grant-role --caller "$ADMIN" --role "$KEEPER_ROLE" --account "$ADMIN"

# ---------------------------------------------------------------------------
# Create test market (market_id 0) via market_factory
# ---------------------------------------------------------------------------
if [[ "$(state_get TEST_MARKET_CREATED)" != "1" ]]; then
  log "creating test market ..."
  # Token addresses must be issued SACs on testnet; use placeholders for CI dry-run.
  INDEX_TOKEN="${TEST_INDEX_TOKEN:-$ADMIN}"
  LONG_TOKEN="${TEST_LONG_TOKEN:-$ADMIN}"
  SHORT_TOKEN="${TEST_SHORT_TOKEN:-$ADMIN}"
  MARKET_TOKEN_ADDR="${TEST_MARKET_TOKEN:-$ADMIN}"

  invoke "$MARKET_FACTORY" create-market \
    --caller "$ADMIN" \
    --index-token "$INDEX_TOKEN" \
    --long-token "$LONG_TOKEN" \
    --short-token "$SHORT_TOKEN" \
    --market-token "$MARKET_TOKEN_ADDR" \
    || log "create-market may need real SAC addresses (set TEST_LONG_TOKEN etc.)"

  invoke "$LIQUIDITY_HANDLER" register-market \
    --caller "$ADMIN" \
    --market-id 0 \
    --long-token "$LONG_TOKEN" \
    --short-token "$SHORT_TOKEN" \
    || true

  state_set TEST_MARKET_CREATED 1
fi

# ---------------------------------------------------------------------------
# Write .env.testnet
# ---------------------------------------------------------------------------
log "writing $ENV_FILE"
cat >"$ENV_FILE" <<EOF
# Generated by scripts/deploy_testnet.sh — $(date -u +%Y-%m-%dT%H:%M:%SZ)
NETWORK=$NETWORK
STELLAR_RPC_URL=$RPC_URL
DEPLOYER=$DEPLOYER
ADMIN=$ADMIN
WASM_HASH=$WASM_HASH

ROLE_STORE=$ROLE_STORE
DATA_STORE=$DATA_STORE
MARKET_TOKEN_WASM_HASH=$WASM_HASH
MARKET_FACTORY=$MARKET_FACTORY
ORACLE=$ORACLE

DEPOSIT_VAULT=$DEPOSIT_VAULT
WITHDRAWAL_VAULT=$WITHDRAWAL_VAULT
ORDER_VAULT=$ORDER_VAULT

DEPOSIT_HANDLER=$DEPOSIT_HANDLER
WITHDRAWAL_HANDLER=$WITHDRAWAL_HANDLER
ORDER_HANDLER=$ORDER_HANDLER
LIQUIDATION_HANDLER=$LIQUIDATION_HANDLER
ADL_HANDLER=$ADL_HANDLER
FEE_HANDLER=$FEE_HANDLER

REFERRAL_STORAGE=$REFERRAL_STORAGE
READER=$READER
EXCHANGE_ROUTER=$EXCHANGE_ROUTER

LIQUIDITY_HANDLER=$LIQUIDITY_HANDLER
POSITION_HANDLER=$POSITION_HANDLER
EOF

log "done — addresses written to $ENV_FILE"
