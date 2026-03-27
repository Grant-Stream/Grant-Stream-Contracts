# PR: Legal Entity Dissolution Auto-Pause (#208)

## Summary
Implements automatic pausing of grant streams when recipient entities are reported as dissolved by an authorized Legal Oracle, protecting DAO treasury from sending funds to legal void entities.

## Features
- **Legal Oracle Integration** - Authorized oracle can report entity status changes
- **Auto-Pause Mechanism** - All active grants to dissolved entities are automatically frozen
- **Status Caching** - 24-hour cache optimizes performance
- **Security Controls** - Authorization and audit trail for all operations
- **Event System** - Comprehensive events for monitoring and compliance

## Key Functions Added
- `set_legal_oracle_contract()` - Admin configures authorized Legal Oracle
- `report_entity_dissolution()` - Oracle reports entity dissolution
- `update_entity_status()` - Oracle updates entity to any status
- `get_entity_status()` - Public query for entity status
- `get_dissolved_entities()` - List all dissolved entities
- `is_entity_dissolved()` - Check if entity is dissolved

## Security
- Only admin can set Legal Oracle address
- Only authorized oracle can report status changes
- Duplicate dissolution reports rejected
- Full audit trail with events

## Testing
Comprehensive test suite covering:
- Oracle setup and authorization
- Entity dissolution and auto-pause
- Multiple grants handling
- Error conditions and edge cases
- Status caching and validation

## Files Changed
- `contracts/grant_contracts/src/lib.rs` - Core implementation
- `contracts/grant_contracts/src/test_legal_entity_monitor.rs` - Test suite
- `LEGAL_ENTITY_DISSOLUTION_FEATURE.md` - Feature documentation

Resolves #208
