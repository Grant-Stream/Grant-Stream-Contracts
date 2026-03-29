# Grant-Stream Contracts

Solidity smart contracts for the Grant-Stream protocol.

## Contracts

- `GrantStream.sol` — Core grant streaming logic with sustainability tax
- `SustainabilityFund.sol` — JerryIdoko Developer Treasury receiver

## Final_Protocol_Sustainability_Fund_Transfer

Once a grant's cumulative claimed volume crosses **100,000 ETH** (proxy for $100,000),
a **0.01% sustainability tax** is applied to every subsequent claim and forwarded to
the `SustainabilityFund` (JerryIdoko Developer Treasury).

- Grants below the threshold pay **zero tax** — free for small builders.
- Claims that straddle the threshold are taxed only on the **above-threshold portion**.
- The treasury address can withdraw accumulated funds at any time.

## Setup

```bash
npm install
npx hardhat compile
npx hardhat test
TREASURY_ADDRESS=0xYourAddress npx hardhat run scripts/deploy.js --network <network>
```
