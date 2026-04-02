# perf: Optimize getStreamStatus to Minimize Ledger Reads

## Summary

Reduces ledger reads for grant status queries from ~5 SLOADs to **1 SLOAD** by introducing a tightly-packed `StreamStatus` cache mapping. Designed for high-traffic "Payday" events where hundreds of grantees query balances simultaneously.

Labels: `optimization` `performance` `backend`

---

## Problem

`getGrantDetails()` loads the full `Grant` struct across ~5 storage slots on every call. For a Super-DAO with 500 active grants under concurrent load, that's **2,500+ ledger reads** just for status checks — driving up gas costs and slowing response times.

---

## Solution

A dedicated `StreamStatus` struct packed into **one 32-byte storage slot**:

```
recipient  : 20 bytes
active     :  1 byte
finalReleaseRequired :  1 byte
finalReleaseApproved :  1 byte
exists     :  1 byte
endDate    :  5 bytes (uint40)
─────────────────────
Total      : 29 bytes → fits in 1 slot
```

Written once at grant creation, updated with targeted field writes on state changes. Reading it costs exactly **1 SLOAD**.

---

## Changes

| File | Change |
|---|---|
| `contracts/GrantStream.sol` | Add `StreamStatus` struct + `streamStatus` mapping |
| `contracts/GrantStream.sol` | Write cache in `createGrant` |
| `contracts/GrantStream.sol` | Sync `active` field in `closeGrant` |
| `contracts/GrantStream.sol` | Sync `finalReleaseApproved` field in `approveFinalRelease` |
| `contracts/GrantStream.sol` | Add `getStreamStatus()` — 1 SLOAD |
| `contracts/GrantStream.sol` | Add `batchGetStreamStatus()` — N SLOADs for N grants |

---

## Gas Impact

| Function | Before | After |
|---|---|---|
| `getStreamStatus(id)` | ~5 SLOADs | **1 SLOAD** |
| `batchGetStreamStatus(ids[])` | ~5N SLOADs | **N SLOADs** |

---

## Usage

```solidity
// Single grant — payday balance check
(address recipient, bool active, , , bool exists, uint40 endDate)
    = grantStream.getStreamStatus(grantId);

// Bulk query — 500 grants in one call
uint256[] memory ids = new uint256[](500);
// ... populate ids ...
GrantStream.StreamStatus[] memory statuses = grantStream.batchGetStreamStatus(ids);
```

---

## Testing Checklist

- [ ] `getStreamStatus()` returns correct values after `createGrant`
- [ ] `active` flips to `false` in cache after `closeGrant`
- [ ] `finalReleaseApproved` flips to `true` in cache after `approveFinalRelease`
- [ ] `batchGetStreamStatus()` returns correct data for all queried IDs
- [ ] Cache and `grants` mapping stay in sync across all state transitions
