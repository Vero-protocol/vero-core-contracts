# Vero Core Contracts — Deployment Guide

This guide walks you through deploying the Vero Core contract to Stellar **testnet**
from scratch. The same steps apply to futurenet and mainnet — only the network flag
and RPC URL change.

---

## Prerequisites

### 1. Install the Rust WASM toolchain

```bash
rustup target add wasm32-unknown-unknown
```

### 2. Install Stellar CLI

```bash
cargo install --locked stellar-cli --features opt
```

> **Note:** The package was renamed from `soroban-cli` to `stellar-cli` in Stellar CLI
> v21. If you already have an older `soroban-cli` binary, the commands below use
> `stellar` — substitute `soroban` if you are on an older version.

Verify the installation:

```bash
stellar --version
```

---

## Environment Variables

Create a `.env` file (never commit it — it is already in `.gitignore`):

```bash
# Network selection: testnet | futurenet | mainnet
NETWORK=testnet

# Your deployer account key pair (generated in the next section)
DEPLOYER_SECRET=S...          # Stellar secret key (starts with S)
DEPLOYER_ADDRESS=G...         # Corresponding public key (starts with G)

# The Stellar token contract to use as the lock token
# On testnet you can deploy a minimal SEP-41 mock, or use the native XLM token.
TOKEN_CONTRACT_ID=C...

# Minimum locked balance a guardian must hold to vote (in stroops for XLM)
LOCK_THRESHOLD=100

# Optional: vault and treasury addresses if integrating escrow/reward flows
VAULT_CONTRACT_ID=C...
TREASURY_ADDRESS=G...
```

Source the file before running commands:

```bash
set -a && source .env && set +a
```

---

## Step 1 — Fund a deployer account via Friendbot (testnet/futurenet only)

Generate a fresh key pair:

```bash
stellar keys generate --global deployer
stellar keys address deployer   # prints the G... public key
```

Fund it using the testnet Friendbot:

```bash
curl "https://friendbot.stellar.org?addr=$(stellar keys address deployer)"
```

For **futurenet**:

```bash
curl "https://friendbot-futurenet.stellar.org?addr=$(stellar keys address deployer)"
```

Verify the account exists on-chain:

```bash
stellar account info \
  --network testnet \
  $(stellar keys address deployer)
```

> **Mainnet note:** There is no Friendbot on mainnet. Fund the account manually
> with at least 2 XLM (base reserve) plus enough to cover transaction fees and
> contract deployment costs (~0.5 XLM).

---

## Step 2 — Build the contract WASM

```bash
make build
# or equivalently:
cargo build --target wasm32-unknown-unknown --release
```

The compiled artifact is produced at:

```
target/wasm32-unknown-unknown/release/vero_core_contracts.wasm
```

You can inspect the exported functions before deploying:

```bash
stellar contract inspect \
  --wasm target/wasm32-unknown-unknown/release/vero_core_contracts.wasm
```

---

## Step 3 — Upload the WASM to the network

Uploading stores the bytecode on-chain and returns a **WASM hash** that you
reference during deployment.

```bash
stellar contract upload \
  --network testnet \
  --source deployer \
  --wasm target/wasm32-unknown-unknown/release/vero_core_contracts.wasm
```

The command prints a 64-character hex WASM hash. Save it:

```bash
export WASM_HASH=<printed-hash>
```

---

## Step 4 — Deploy a contract instance

```bash
stellar contract deploy \
  --network testnet \
  --source deployer \
  --wasm-hash $WASM_HASH
```

This prints the new **contract ID** (starts with `C`). Save it:

```bash
export CONTRACT_ID=<printed-contract-id>
```

> You can also pass `--salt <hex>` to produce a deterministic contract address
> for reproducible deployments across environments.

---

## Step 5 — Initialize the contract

The `initialize` call sets the admin, the token contract used for guardian locking,
and the minimum locked balance threshold. It must be called exactly once.

```bash
stellar contract invoke \
  --network testnet \
  --source deployer \
  --id $CONTRACT_ID \
  -- initialize \
  --admin $(stellar keys address deployer) \
  --token $TOKEN_CONTRACT_ID \
  --lock_threshold $LOCK_THRESHOLD
```

Verify initialization succeeded by reading the admin back:

```bash
stellar contract invoke \
  --network testnet \
  --source deployer \
  --id $CONTRACT_ID \
  -- get_admin
```

---

## Step 6 — Configure roles

The contract uses role-based access control. The deployer address starts with the
`Admin` role. Grant additional roles to operational accounts before handing off
duties:

```bash
# Grant GuardianManager role (can add/remove guardians)
stellar contract invoke \
  --network testnet --source deployer --id $CONTRACT_ID \
  -- grant_role \
  --caller $(stellar keys address deployer) \
  --target $GUARDIAN_MANAGER_ADDRESS \
  --role '{"GuardianManager": null}'

# Grant TaskManager role (can register/cancel tasks)
stellar contract invoke \
  --network testnet --source deployer --id $CONTRACT_ID \
  -- grant_role \
  --caller $(stellar keys address deployer) \
  --target $TASK_MANAGER_ADDRESS \
  --role '{"TaskManager": null}'

# Grant ConfigManager role (threshold, vault, fee config)
stellar contract invoke \
  --network testnet --source deployer --id $CONTRACT_ID \
  -- grant_role \
  --caller $(stellar keys address deployer) \
  --target $CONFIG_MANAGER_ADDRESS \
  --role '{"ConfigManager": null}'

# Grant EmergencyManager role (pause/unpause, circuit breaker reset)
stellar contract invoke \
  --network testnet --source deployer --id $CONTRACT_ID \
  -- grant_role \
  --caller $(stellar keys address deployer) \
  --target $EMERGENCY_MANAGER_ADDRESS \
  --role '{"EmergencyManager": null}'
```

---

## Step 7 — Add guardians and set reputation

```bash
# Whitelist a guardian address (requires GuardianManager role)
stellar contract invoke \
  --network testnet --source deployer --id $CONTRACT_ID \
  -- add_guardian \
  --admin $(stellar keys address deployer) \
  --guardian $GUARDIAN_ADDRESS

# Assign a reputation score (voting weight)
stellar contract invoke \
  --network testnet --source deployer --id $CONTRACT_ID \
  -- set_reputation \
  --admin $(stellar keys address deployer) \
  --guardian $GUARDIAN_ADDRESS \
  --score 300
```

Default weight threshold for task resolution is **300**. A single guardian with
score 300 can resolve a task alone. Adjust the threshold to require multiple
guardians:

```bash
stellar contract invoke \
  --network testnet --source deployer --id $CONTRACT_ID \
  -- set_weight_threshold \
  --admin $(stellar keys address deployer) \
  --threshold 900
```

---

## Step 8 — Guardian token locking

Before a guardian can vote, they must lock tokens above the `lock_threshold`.

```bash
# Guardian locks tokens (invoked from the guardian's key)
stellar contract invoke \
  --network testnet --source guardian-key --id $CONTRACT_ID \
  -- lock_tokens \
  --guardian $GUARDIAN_ADDRESS \
  --amount 150
```

---

## Step 9 — Register a task and cast votes

```bash
# Register a GitHub PR as a task (task_id = PR number, min_votes_required = 1)
stellar contract invoke \
  --network testnet --source deployer --id $CONTRACT_ID \
  -- register_task \
  --admin $(stellar keys address deployer) \
  --task_id 42 \
  --min_votes_required 1

# Guardian votes on the task
stellar contract invoke \
  --network testnet --source guardian-key --id $CONTRACT_ID \
  -- vote \
  --guardian $GUARDIAN_ADDRESS \
  --task_id 42

# Check whether the task is resolved
stellar contract invoke \
  --network testnet --source deployer --id $CONTRACT_ID \
  -- get_task \
  --task_id 42
```

When `total_weight_accrued >= weight_threshold` the response will show `"is_done": true`.

---

## Step 10 — Multi-sig upgrade setup (recommended for production)

Before going to mainnet, configure a multi-sig quorum for contract upgrades to
prevent single-key takeover.

```bash
# Require 2-of-3 signers to approve any upgrade
stellar contract invoke \
  --network testnet --source deployer --id $CONTRACT_ID \
  -- set_upgrade_signers \
  --admin $(stellar keys address deployer) \
  --signers "[$SIGNER_A, $SIGNER_B, $SIGNER_C]" \
  --threshold 2
```

> **Important:** Signers must be provided in **strictly ascending lexicographic
> order** of their address strings. The contract enforces this to prevent
> duplicate entries.

To propose and execute an upgrade later:

```bash
# Signer A proposes a new WASM hash
stellar contract invoke \
  --network testnet --source signer-a --id $CONTRACT_ID \
  -- propose_upgrade \
  --signer $SIGNER_A \
  --new_wasm_hash $NEW_WASM_HASH

# Signer B approves
stellar contract invoke \
  --network testnet --source signer-b --id $CONTRACT_ID \
  -- approve_upgrade \
  --signer $SIGNER_B

# Anyone calls execute_upgrade once the threshold is met
stellar contract invoke \
  --network testnet --source deployer --id $CONTRACT_ID \
  -- execute_upgrade
```

---

## Network Reference

| Network | RPC URL | Friendbot | Explorer |
|---|---|---|---|
| Testnet | `https://soroban-testnet.stellar.org` | `https://friendbot.stellar.org` | `https://stellar.expert/explorer/testnet` |
| Futurenet | `https://rpc-futurenet.stellar.org` | `https://friendbot-futurenet.stellar.org` | `https://stellar.expert/explorer/futurenet` |
| Mainnet | `https://soroban-mainnet.stellar.org` | — | `https://stellar.expert/explorer/public` |

To target a specific RPC directly instead of using the `--network` shorthand:

```bash
stellar contract invoke \
  --rpc-url https://soroban-testnet.stellar.org \
  --network-passphrase "Test SDF Network ; September 2015" \
  --source deployer \
  --id $CONTRACT_ID \
  -- get_admin
```

Mainnet passphrase: `"Public Global Stellar Network ; September 2015"`
Futurenet passphrase: `"Test SDF Future Network ; October 2022"`

---

## Makefile shortcut

The repo's `Makefile` includes a `deploy-testnet` target. You can use it as a
convenience wrapper once you have the environment variables set:

```bash
make build
make deploy-testnet
```

---

## Post-deploy checklist

- [ ] `get_admin` returns the expected admin address
- [ ] `is_paused` returns `false`
- [ ] At least one guardian added and reputation score set
- [ ] Guardian has locked tokens above `lock_threshold`
- [ ] `get_weight_threshold` returns the intended value
- [ ] Multi-sig signers configured via `set_upgrade_signers` (production only)
- [ ] Vault address set if using escrow flows (`set_vault_address`)
- [ ] Contract ID recorded in your deployment log / CI environment variables

---

## Troubleshooting

**`AlreadyInitialized` (error 7)** — `initialize` was called twice. Each
contract instance can only be initialized once. Deploy a new instance if you
need a fresh state.

**`NotAuthorized` (error 1)** — The caller does not hold the required role.
Use `has_role` to confirm role assignments, and `grant_role` to assign missing ones.

**`InsufficientLockedBalance` (error 9)** — The guardian's locked balance does
not exceed `lock_threshold`. Call `lock_tokens` with an amount strictly greater
than the threshold before voting.

**`ContractPaused` (error 15)** — All state-changing calls are blocked. An
`EmergencyManager` must call `unpause` or `reset_circuit_breaker` to resume.

**`InvalidUpgradeConfig` (error 34)** — Upgrade signers must be provided in
strictly ascending address order and the threshold must be ≥ 1 and ≤ number of
signers.

**TTL expiry** — Soroban instance storage has a finite TTL. The contract extends
it by 100 000 ledgers on every guardian write, but if the contract goes dormant
for a long period, storage entries may expire. Call any write operation (or use
`stellar contract extend`) to bump the TTL before it lapses.
