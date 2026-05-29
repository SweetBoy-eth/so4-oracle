.PHONY: dev deploy-local test build-contracts

COMPOSE := docker compose
ROOT := $(shell pwd)

dev:
	$(COMPOSE) up -d --build
	@echo "Services starting (Stellar :8000, Redis :6379, APIs :3000)"
	@echo "Run 'make deploy-local' after quickstart is healthy."

deploy-local:
	NETWORK=local \
	STELLAR_RPC_URL=http://localhost:8000/soroban/rpc \
	DEPLOYER=local-deployer \
	./scripts/deploy_testnet.sh

test:
	cargo test --all
	@echo "For Soroban contract tests only: cargo test -p contracts"

build-contracts:
	./scripts/build_contracts.sh

down:
	$(COMPOSE) down
