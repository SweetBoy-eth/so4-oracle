#!/usr/bin/env bash
# Build the contracts WASM artifact for deployment.
#
# Note: this crate bundles multiple Soroban contracts. A single cdylib build
# requires unique exported symbols per contract; until split crates land upstream,
# use a pre-built WASM from CI artifacts or set CONTRACTS_WASM manually.
#
# Usage: ./scripts/build_contracts.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${CONTRACTS_WASM:-$ROOT/target/wasm32v1-none/release/contracts.wasm}"

log() { printf '[build] %s\n' "$*"; }

if [[ -f "$OUT" && "${FORCE_REBUILD:-0}" != "1" ]]; then
  log "WASM already exists at $OUT"
  exit 0
fi

log "building contracts (stellar contract build) ..."
if stellar contract build --manifest-path "$ROOT/contracts/Cargo.toml" 2>&1; then
  log "built successfully"
  exit 0
fi

log "WARN: unified WASM build failed (multi-contract symbol collision)."
log "      Tests still pass via rlib: cargo test -p contracts"
log "      For testnet deploy, provide a pre-built WASM:"
log "        export CONTRACTS_WASM=/path/to/contracts.wasm"
log "      Or split contracts into separate crates (see README)."
exit 1
