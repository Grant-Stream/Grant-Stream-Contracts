# Grant-Stream Contracts

Soroban smart contracts for streaming grant payments on Stellar. Supports fixed-amount and fixed-end-date streaming, warmup periods, KPI multipliers, rate changes with timelocks, rage quit, and validator incentives (5% ecosystem tax).

## Architecture

The workspace contains two contract crates:

- **grant_contracts** — Core grant streaming logic: create grants, withdraw, pause/resume, propose rate changes, apply KPI multipliers, rage quit, cancel, rescue tokens, and validator split (95/5).
- **vesting_contracts** — Placeholder contract template.

## Prerequisites

- [Rust](https://rustup.rs/) (stable) with `wasm32-unknown-unknown` target
- Soroban SDK (v22.0.0)

## Setup

```bash
# Add WASM target
rustup target add wasm32-unknown-unknown

# Build all contracts
cargo build

# Run tests
cargo test
```

## Testing

```bash
# Run all workspace tests
cargo test

# Run only grant contract tests
cargo test -p grant_contracts

# Run only vesting contract tests
cargo test -p vesting_contracts
```

## Key Features

- **Grant Streaming**: Create grants with fixed flow rates, warmup periods, and timelocked rate increases.
- **KPI Multipliers**: Oracle-authorized function to scale rates based on on-chain performance metrics.
- **Validator Incentives**: Optional validator address receives 5% of streamed tokens (ecosystem tax).
- **Rage Quit**: Grantee can quit a paused grant, claiming accrued funds and returning remaining to treasury.
- **Cancel**: Admin can cancel grants; unstreamed funds return to treasury.
- **Rescue Tokens**: Admin can rescue excess tokens while preserving allocated funds.
