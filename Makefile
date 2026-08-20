# Vero Core Contracts — Makefile
#
# Targets:
#   initialize-testnet  - Initialize contract on testnet
#   build            — Compile the WASM contract
#   test             — Run all unit and integration tests
#   check            — Quick syntax check without full compilation
#   verify           — Run Kani formal verification harnesses
#   invariants       — Run runtime safety invariant tests
#   proofs           — Run K-framework proofs on Linux/macOS (requires K Framework)
#   proofs-windows   — Run K-framework proofs on Windows via PowerShell (requires K Framework)
#   deploy-testnet   — Upload WASM and deploy a fresh instance on testnet
#   all              — Build, test, and verify

.PHONY: build test check verify invariants proofs proofs-windows deploy-testnet all

build:
	cargo build --target wasm32-unknown-unknown --release

test:
	cargo test

check:
	cargo check

# Kani proof harnesses (requires cargo-kani installed)
verify:
	cargo kani --manifest-path verification/Cargo.toml

# Runtime invariant tests (pure consensus logic, no Soroban host)
invariants:
	cargo test --test safety_invariants

# K-framework proofs (requires K Framework 6.0+)
proofs:
	cd proofs && bash build.sh

# K-framework proofs — Windows entry point (requires K Framework 6.0+ and PowerShell)
proofs-windows:
	powershell -ExecutionPolicy Bypass -File proofs/build.ps1

# Deploy to testnet.
# Requires: DEPLOYER_SECRET, TOKEN_CONTRACT_ID, and LOCK_THRESHOLD to be set
# in the environment (or sourced from .env).  See DEPLOY.md for full setup.
deploy-testnet: build
	@echo "==> Uploading WASM to testnet..."
	$(eval WASM_HASH := $(shell stellar contract upload \
		--network testnet \
		--source deployer \
		--wasm target/wasm32-unknown-unknown/release/vero_core_contracts.wasm))
	@echo "WASM hash: $(WASM_HASH)"
	@echo "==> Deploying contract instance..."
	$(eval CONTRACT_ID := $(shell stellar contract deploy \
		--network testnet \
		--source deployer \
		--wasm-hash $(WASM_HASH)))
	@echo "Contract ID: $(CONTRACT_ID)"
	@echo ""
	@echo "Next: run 'make initialize-testnet CONTRACT_ID=$(CONTRACT_ID)' or follow DEPLOY.md Step 5."

# Initialize a deployed contract instance on testnet.
# Requires: CONTRACT_ID, TOKEN_CONTRACT_ID, LOCK_THRESHOLD env vars.
initialize-testnet:
	stellar contract invoke \
		--network testnet \
		--source deployer \
		--id $(CONTRACT_ID) \
		-- initialize \
		--admin $$(stellar keys address deployer) \
		--token $(TOKEN_CONTRACT_ID) \
		--lock_threshold $(LOCK_THRESHOLD)

all: check test invariants verify