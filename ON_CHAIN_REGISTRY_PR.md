# On-Chain Grant Registry Index Implementation

## Summary

This PR implements the **On-Chain Grant Registry Index** feature, which enables Meta-DAOs and Ecosystem Dashboards to dynamically browse all grants from within other smart contracts without relying on centralized off-chain databases.

## Problem Solved

Previously, there was no way for external contracts to systematically discover and query all grants managed by the Grant-Stream contract. This limitation prevented:

- Meta-DAOs from tracking ecosystem-wide funding activity
- Ecosystem dashboards from displaying comprehensive grant statistics  
- Cross-contract interoperability for grant analytics
- Decentralized grant discovery and indexing

## Solution

### Core Implementation

1. **Grant Registry Storage**: Added `DataKey::GrantRegistry(Address)` to track grants by landlord (lessor) address
2. **list_grants_by_landlord Function**: Returns array of contract hashes for all grants associated with a given landlord
3. **Grant Registry Helper Functions**: 
   - `read_grant_registry()` - Retrieve grant hashes for a landlord
   - `write_grant_registry()` - Store grant hashes for a landlord
   - `add_grant_to_registry()` - Add new grant to registry
   - `generate_grant_hash()` - Create deterministic grant identifiers

### Enhanced Features

4. **GrantRegistryStats Type**: Comprehensive statistics structure including:
   - Total grants count
   - Active/completed/paused/cancelled breakdown
   - Total amount locked
   - Last updated timestamp

5. **get_grant_registry_stats Function**: Provides detailed analytics for ecosystem dashboards

6. **Automatic Registration**: Updated `batch_init_with_deposits` to automatically register new grants

## Key Benefits

- **Decentralized Discovery**: No reliance on external databases or oracles
- **Real-time Analytics**: Live grant statistics and activity tracking
- **Meta-DAO Integration**: Enables DAO governance to monitor ecosystem funding
- **Ecosystem Dashboards**: Powers comprehensive funding activity displays
- **Contract Interoperability**: Allows smart contracts to query grant data

## Technical Details

### Storage Structure
```
GrantRegistry(Address) -> Vec<[u8; 32]>  // Landlord -> Grant Hashes
```

### Function Signatures
```rust
pub fn list_grants_by_landlord(env: Env, landlord: Address) -> Vec<[u8; 32]>
pub fn get_grant_registry_stats(env: Env, landlord: Option<Address>) -> GrantRegistryStats
```

### Grant Hash Generation
Deterministic hash generation using grant_id and contract_address for unique identification.

## Usage Examples

### Meta-DAO Query
```rust
// Get all grants for a specific landlord
let landlord_grants = contract.list_grants_by_landlord(env, landlord_address);
```

### Ecosystem Dashboard Stats
```rust
// Get global grant statistics
let global_stats = contract.get_grant_registry_stats(env, None);

// Get landlord-specific statistics  
let landlord_stats = contract.get_grant_registry_stats(env, Some(landlord_address));
```

## Gas Efficiency

- Storage optimized with minimal additional data structures
- Efficient vector-based storage for grant hashes
- Lazy evaluation for statistics calculation
- No gas overhead for grants not using the registry

## Backward Compatibility

- Fully backward compatible with existing contracts
- No changes to existing grant storage structures
- Optional feature - only affects newly created grants
- Existing grants can be retroactively indexed via admin functions

## Testing

- Implementation follows existing contract patterns
- Uses established helper function patterns
- Compatible with existing test infrastructure
- Ready for integration testing

## Labels

- `interop` - Enables cross-contract communication
- `backend` - Core infrastructure enhancement  
- `smart-contract` - Smart contract implementation

## Related Issues

Resolves: **On-Chain_Grant_Registry_Index**

## Next Steps

1. Integration testing with Meta-DAO contracts
2. Ecosystem dashboard integration examples
3. Documentation for external developers
4. Performance optimization for large-scale deployments
