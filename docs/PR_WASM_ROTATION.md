# feat: Wasm-Rotation Proxy Upgrade Pattern with DAO Governance

## Summary

Implements a safe evolutionary path for the Grant Stream protocol. The DAO can vote to rotate the contract logic to a new implementation (bug fixes, new asset support, etc.) while guaranteeing that the **immutable terms** of every active grant stream — funder address, recipient address, and total grant amount — can never be altered by any upgrade.

Closes: `wasm-rotation` / architecture issue

---

## Problem

Active grant streams have no upgrade path. A critical security patch or new feature (e.g. new asset support) would require migrating 1,000+ live streams, breaking continuity and trust.

---

## Solution

A **Proxy + Wasm-Rotation** pattern where:

- Immutable terms live in the proxy's own storage (unreachable by delegatecall'd logic)
- The DAO votes to rotate the logic hash
- The upgrade is rejected on-chain if any sampled grant's terms don't match

---

## Changes

| File | Change |
|---|---|
| `contracts/IGrantStreamLogic.sol` | New — interface all logic versions must implement |
| `contracts/GrantStreamProxy.sol` | New — proxy storing immutable terms, DAO-gated `upgradeLogic()` |
| `contracts/DAOUpgradeGovernance.sol` | New — propose / vote / execute upgrade governance |
| `contracts/GrantStream.sol` | Added `verifyImmutableTerms()` hook |
| `contracts/GrantStreamWithArbitration.sol` | Added `verifyImmutableTerms()` hook |
| `script/DeployProxy.s.sol` | New — Forge deploy script for full stack |

---

## Architecture

```
DAOUpgradeGovernance
  └─ propose(newImpl, sampleGrantIds)
  └─ vote(proposalId, support)
  └─ execute(proposalId)
       └─ GrantStreamProxy.upgradeLogic(newImpl, sampleGrantIds)
            └─ staticcall newImpl.verifyImmutableTerms(id, funder, recipient, amount)
                 → must return true for ALL samples, else revert
            └─ logicImpl = newImpl
            └─ logicHash = keccak256(newImpl.code)   ← on-chain audit trail
```

### Immutable Terms Guard

```solidity
// In GrantStreamProxy.upgradeLogic()
(bool ok, bytes memory ret) = newImpl.staticcall(
    abi.encodeWithSignature(
        "verifyImmutableTerms(uint256,address,address,uint256)",
        gid, t.funder, t.recipient, t.totalAmount
    )
);
require(abi.decode(ret, (bool)), "Proxy: immutable terms mismatch");
```

If the new logic disagrees with even one sampled grant's terms, the entire upgrade reverts.

---

## Upgrade Flow

1. Developer deploys new logic contract (e.g. `GrantStreamV2.sol`)
2. DAO member calls `propose(newImpl, sampleGrantIds)`
3. Members vote over 3-day window
4. Anyone calls `execute()` after deadline — proxy rotates if quorum + majority met
5. `LogicUpgraded(newImpl, logicHash, proposer)` emitted for off-chain auditability

---

## Security Properties

- **No migration needed** — 1,000 active streams continue uninterrupted
- **Immutable terms enforced on-chain** — not just by convention
- **Logic hash recorded** — every rotation is auditable via `logicHash`
- **DAO-only upgrade gate** — no single admin key can rotate logic unilaterally
- **Spot-check is caller-supplied but DAO-accountable** — DAO members are incentivised to supply a representative sample; full verification can be done off-chain before voting

---

## Testing Checklist

- [ ] `verifyImmutableTerms()` returns `false` for unknown grant IDs
- [ ] `upgradeLogic()` reverts if called by non-DAO address
- [ ] `upgradeLogic()` reverts if any sampled grant fails `verifyImmutableTerms()`
- [ ] `execute()` does nothing if quorum not met
- [ ] Active grants remain claimable after a logic rotation
