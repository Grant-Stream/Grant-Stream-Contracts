#![cfg(test)]

use soroban_sdk::{symbol_short, Address, Env, Vec, Map, String};
use crate::interest_redirection::{
    InterestRedirectionContract, InterestRedirectionClient, InterestRedirectionError,
    BurnConfig, YieldToBurnOperation, BurnOperationStatus, TokenSupplyMetrics,
    InterestRedirectionDataKey, DEAD_ADDRESS, DEFAULT_BURN_RATIO, MIN_BURN_RATIO,
    MAX_BURN_RATIO, BURN_EXECUTION_INTERVAL, MIN_YIELD_THRESHOLD,
};

#[test]
fn test_interest_redirection_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    // Initialize with valid parameters
    let result = client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO, // 50% burn ratio
        &true, // Auto-burn enabled
    );
    
    assert!(result.is_ok());
    
    // Verify configuration
    let config = client.get_config().unwrap();
    assert_eq!(config.admin, admin);
    assert_eq!(config.project_token, project_token);
    assert_eq!(config.burn_ratio, DEFAULT_BURN_RATIO);
    assert!(config.auto_burn_enabled);
    assert_eq!(config.burn_interval, BURN_EXECUTION_INTERVAL);
    assert_eq!(config.min_yield_threshold, MIN_YIELD_THRESHOLD);
    assert_eq!(config.last_burn_amount, 0);
    assert_eq!(config.total_burned, 0);
    assert_eq!(config.burn_count, 0);
    
    // Verify dead address was set
    let dead_address = Address::from_string(&env, DEAD_ADDRESS);
    // This would be verified through contract storage in real implementation
}

#[test]
fn test_interest_redirection_invalid_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    // Test with invalid burn ratio (< 10%)
    let result = client.try_initialize(
        &admin,
        &project_token,
        &500u32, // 5% - too low
        &true,
    );
    assert_eq!(result, Err(InterestRedirectionError::InvalidBurnRatio));
    
    // Test with invalid burn ratio (> 90%)
    let result = client.try_initialize(
        &admin,
        &project_token,
        &9500u32, // 95% - too high
        &true,
    );
    assert_eq!(result, Err(InterestRedirectionError::InvalidBurnRatio));
    
    // Test double initialization
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    let result = client.try_initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    );
    assert_eq!(result, Err(InterestRedirectionError::AlreadyExists));
}

#[test]
fn test_create_burn_operation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    // Initialize
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    // Create burn operation
    let operation_id = client.create_burn_operation(
        1u64, // grant_id
        &10000u128, // yield amount
        &5000u128, // burn amount (50%)
        &500u32, // 5% slippage tolerance
    ).unwrap();
    
    assert_eq!(operation_id, 1); // First operation should have ID 1
    
    // Verify operation details
    let operation = client.get_burn_operation(operation_id).unwrap();
    assert_eq!(operation.operation_id, operation_id);
    assert_eq!(operation.grant_id, 1);
    assert_eq!(operation.yield_amount, 10000);
    assert_eq!(operation.burn_amount, 5000);
    assert_eq!(operation.slippage_tolerance, 500);
    assert_eq!(operation.status, BurnOperationStatus::Pending);
    assert!(operation.executed_at.is_none());
}

#[test]
fn test_create_burn_operation_invalid() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    // Test with insufficient yield
    let result = client.try_create_burn_operation(
        1u64,
        &500u128, // Below minimum threshold
        &250u128,
        &500u32,
    );
    assert_eq!(result, Err(InterestRedirectionError::InsufficientYield));
    
    // Test with zero burn amount
    let result = client.try_create_burn_operation(
        1u64,
        &10000u128,
        &0u128, // Invalid
        &500u32,
    );
    assert_eq!(result, Err(InterestRedirectionError::InvalidAmount));
}

#[test]
fn test_execute_burn_operation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    // Create burn operation
    let operation_id = client.create_burn_operation(
        1u64,
        &10000u128,
        &5000u128,
        &500u32,
    ).unwrap();
    
    // Execute burn operation
    let result = client.execute_burn_operation(operation_id);
    assert!(result.is_ok());
    
    // Verify operation was executed
    let operation = client.get_burn_operation(operation_id).unwrap();
    assert_eq!(operation.status, BurnOperationStatus::Completed);
    assert!(operation.executed_at.is_some());
    assert!(operation.actual_burned > 0);
    assert!(operation.gas_used > 0);
    
    // Verify token supply metrics were updated
    let metrics = client.get_token_supply_metrics().unwrap();
    assert!(metrics.total_burned > 0);
    assert!(metrics.last_burn_timestamp > 0);
}

#[test]
fn test_execute_burn_operation_invalid_state() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    let operation_id = client.create_burn_operation(
        1u64,
        &10000u128,
        &5000u128,
        &500u32,
    ).unwrap();
    
    // Execute same operation again (should fail)
    let result = client.try_execute_burn_operation(operation_id);
    assert_eq!(result, Err(InterestRedirectionError::InvalidOperationState));
}

#[test]
fn test_auto_burn_execution() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    // Create burn operation manually to accumulate yield
    let _ = client.create_burn_operation(
        1u64,
        &20000u128, // Large yield amount
        &10000u128, // Large burn amount
        &500u32,
    ).unwrap();
    
    // Execute auto-burn
    let executed_operations = client.execute_auto_burn();
    assert!(executed_operations.len() > 0);
    
    // Verify yield accumulator was cleared
    let config = client.get_config().unwrap();
    // This would be verified through yield accumulator storage
}

#[test]
fn test_auto_burn_disabled() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    // Initialize with auto-burn disabled
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &false, // Auto-burn disabled
    ).unwrap();
    
    // Try to execute auto-burn
    let result = client.try_execute_auto_burn();
    assert_eq!(result, Err(InterestRedirectionError::AutoBurnDisabled));
}

#[test]
fn test_auto_burn_insufficient_yield() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    // Try auto-burn with insufficient accumulated yield
    let executed_operations = client.execute_auto_burn();
    assert_eq!(executed_operations.len(), 0); // No operations should be executed
}

#[test]
fn test_auto_burn_timing() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    // Create burn operation
    let _ = client.create_burn_operation(
        1u64,
        &20000u128,
        &10000u128,
        &500u32,
    ).unwrap();
    
    // Try auto-burn immediately (should fail due to timing)
    let executed_operations = client.execute_auto_burn();
    assert_eq!(executed_operations.len(), 0);
    
    // Advance time past burn interval
    env.ledger().set_timestamp(env.ledger().timestamp() + BURN_EXECUTION_INTERVAL + 1);
    
    // Try auto-burn again (should succeed)
    let executed_operations = client.execute_auto_burn();
    assert!(executed_operations.len() > 0);
}

#[test]
fn test_update_burn_config() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    // Update burn ratio
    let result = client.update_burn_config(
        &admin,
        Some(7000u32), // 70% burn ratio
        Some(false),      // Disable auto-burn
        None,             // Keep default interval
        None,             // Keep default threshold
    );
    assert!(result.is_ok());
    
    // Verify configuration was updated
    let config = client.get_config().unwrap();
    assert_eq!(config.burn_ratio, 7000);
    assert!(!config.auto_burn_enabled);
}

#[test]
fn test_update_burn_config_unauthorized() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    // Try to update config with unauthorized user
    let result = client.try_update_burn_config(
        &unauthorized,
        Some(7000u32),
        Some(false),
        None,
        None,
    );
    assert_eq!(result, Err(InterestRedirectionError::Unauthorized));
}

#[test]
fn test_update_burn_config_invalid_ratio() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    // Try to update with invalid burn ratio (< 10%)
    let result = client.try_update_burn_config(
        &admin,
        Some(500u32), // 5% - too low
        Some(true),
        None,
        None,
    );
    assert_eq!(result, Err(InterestRedirectionError::InvalidBurnRatio));
}

#[test]
fn test_get_pending_operations() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    // Create multiple burn operations
    let op1_id = client.create_burn_operation(
        1u64,
        &10000u128,
        &5000u128,
        &500u32,
    ).unwrap();
    
    let op2_id = client.create_burn_operation(
        2u64,
        &15000u128,
        &7500u128,
        &500u32,
    ).unwrap();
    
    // Get pending operations
    let pending = client.get_pending_operations().unwrap();
    assert_eq!(pending.len(), 2);
    assert!(pending.contains(&op1_id));
    assert!(pending.contains(&op2_id));
    
    // Execute first operation
    client.execute_burn_operation(op1_id).unwrap();
    
    // Check pending operations (should only contain op2)
    let pending_after = client.get_pending_operations().unwrap();
    assert_eq!(pending_after.len(), 1);
    assert!(pending_after.contains(&op2_id));
    assert!(!pending_after.contains(&op1_id));
}

#[test]
fn test_token_supply_metrics() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    // Check initial metrics
    let metrics = client.get_token_supply_metrics().unwrap();
    assert!(metrics.initial_supply > 0);
    assert_eq!(metrics.current_supply, metrics.initial_supply);
    assert_eq!(metrics.total_burned, 0);
    assert_eq!(metrics.total_yield_generated, 0);
    assert_eq!(metrics.last_burn_timestamp, 0);
    assert_eq!(metrics.burn_rate, DEFAULT_BURN_RATIO);
    assert_eq!(metrics.yield_to_burn_ratio, DEFAULT_BURN_RATIO);
    
    // Create and execute burn operation
    let operation_id = client.create_burn_operation(
        1u64,
        &10000u128,
        &5000u128,
        &500u32,
    ).unwrap();
    
    client.execute_burn_operation(operation_id).unwrap();
    
    // Check updated metrics
    let updated_metrics = client.get_token_supply_metrics().unwrap();
    assert!(updated_metrics.total_burned > 0);
    assert!(updated_metrics.last_burn_timestamp > 0);
    assert!(updated_metrics.current_supply < metrics.current_supply);
}

#[test]
fn test_edge_cases() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    // Initialize with minimum values
    client.initialize(
        &admin,
        &project_token,
        &MIN_BURN_RATIO, // 10% minimum
        &true,
    ).unwrap();
    
    // Initialize with maximum values
    let admin2 = Address::generate(&env);
    let project_token2 = Address::generate(&env);
    let contract_id2 = env.register_contract(None, InterestRedirectionContract);
    let client2 = InterestRedirectionClient::new(&env, &contract_id2);
    
    client2.initialize(
        &admin2,
        &project_token2,
        &MAX_BURN_RATIO, // 90% maximum
        &true,
    ).unwrap();
    
    // Test with maximum burn amount
    let max_operation_id = client2.create_burn_operation(
        1u64,
        &u128::MAX, // Maximum yield amount
        &u128::MAX, // Maximum burn amount
        &1000u32, // 10% slippage tolerance
    ).unwrap();
    
    let max_operation = client2.get_burn_operation(max_operation_id).unwrap();
    assert_eq!(max_operation.yield_amount, u128::MAX);
    assert_eq!(max_operation.burn_amount, u128::MAX);
    assert_eq!(max_operation.slippage_tolerance, 1000);
    
    // Test with minimum slippage tolerance
    let min_slippage_id = client.create_burn_operation(
        2u64,
        &10000u128,
        &5000u128,
        &0u32, // 0% slippage tolerance
    ).unwrap();
    
    let min_slippage_op = client2.get_burn_operation(min_slippage_id).unwrap();
    assert_eq!(min_slippage_op.slippage_tolerance, 0);
}

#[test]
fn test_error_conditions() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let project_token = Address::generate(&env);
    let contract_id = env.register_contract(None, InterestRedirectionContract);
    let client = InterestRedirectionClient::new(&env, &contract_id);
    
    // Test operations on uninitialized contract
    let result = client.try_get_config();
    assert_eq!(result, Err(InterestRedirectionError::NotInitialized));
    
    let result = client.try_get_burn_operation(1u64);
    assert_eq!(result, Err(InterestRedirectionError::BurnOperationNotFound));
    
    let result = client.try_get_token_supply_metrics();
    assert_eq!(result, Err(InterestRedirectionError::NotInitialized));
    
    let result = client.try_get_pending_operations();
    assert_eq!(result, Err(InterestRedirectionError::NotInitialized));
    
    // Initialize and test other error conditions
    client.initialize(
        &admin,
        &project_token,
        &DEFAULT_BURN_RATIO,
        &true,
    ).unwrap();
    
    // Test getting non-existent operation
    let result = client.try_get_burn_operation(999u64);
    assert_eq!(result, Err(InterestRedirectionError::BurnOperationNotFound));
}
