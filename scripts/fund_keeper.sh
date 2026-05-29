#!/usr/bin/env bash
# fund_keeper.sh — Top up the SO4 keeper account via Stellar Friendbot (Issue #120).
#
# Usage:
#   ./scripts/fund_keeper.sh <KEEPER_ADDRESS>
#
# Environment:
#   STELLAR_NETWORK   testnet | mainnet  (default: testnet)
#
# Behaviour:
#   testnet  — calls Friendbot and exits 0 on success (idempotent: 400 is ok).
#   mainnet  — logs a warning to stderr and exits 0 (never auto-funds mainnet).
#
# Manual smoke test against testnet Friendbot:
#   STELLAR_NETWORK=testnet ./scripts/fund_keeper.sh GABJLI4QBS2VJHJ2RCJIVXRR...
#
# Note: Friendbot returns HTTP 400 if the account already has funds — that is
# treated as success (idempotent).

set -euo pipefail

KEEPER_ADDRESS="${1:-}"
NETWORK="${STELLAR_NETWORK:-testnet}"
FRIENDBOT_URL="https://friendbot.stellar.org"

# ── Validation ────────────────────────────────────────────────────────────────

if [[ -z "$KEEPER_ADDRESS" ]]; then
  echo "[fund_keeper] ERROR: keeper address is required" >&2
  echo "Usage: $0 <KEEPER_ADDRESS>" >&2
  exit 1
fi

if [[ ! "$KEEPER_ADDRESS" =~ ^G[A-Z2-7]{55}$ ]]; then
  echo "[fund_keeper] ERROR: '$KEEPER_ADDRESS' does not look like a valid Stellar public key" >&2
  exit 1
fi

# ── Mainnet guard ─────────────────────────────────────────────────────────────

if [[ "$NETWORK" == "mainnet" ]]; then
  echo "[fund_keeper] ALERT: keeper account $KEEPER_ADDRESS is below minimum balance on MAINNET." >&2
  echo "[fund_keeper] Auto-funding is disabled on mainnet — please top up the account manually." >&2
  exit 0
fi

# ── Testnet: call Friendbot ───────────────────────────────────────────────────

echo "[fund_keeper] Requesting testnet XLM for $KEEPER_ADDRESS ..."

HTTP_STATUS=$(
  curl -sf -o /dev/null -w "%{http_code}" \
    "${FRIENDBOT_URL}?addr=${KEEPER_ADDRESS}" \
  || true
)

case "$HTTP_STATUS" in
  200)
    echo "[fund_keeper] ✓ Friendbot funded $KEEPER_ADDRESS (HTTP 200)"
    ;;
  400)
    # 400 = account already exists / already funded — treat as success
    echo "[fund_keeper] ✓ Friendbot returned 400 for $KEEPER_ADDRESS (already funded — ok)"
    ;;
  "")
    echo "[fund_keeper] ERROR: curl failed — check your network connection" >&2
    exit 1
    ;;
  *)
    echo "[fund_keeper] ERROR: Friendbot returned unexpected HTTP $HTTP_STATUS for $KEEPER_ADDRESS" >&2
    exit 1
    ;;
esac
