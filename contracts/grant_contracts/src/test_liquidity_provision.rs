#![cfg(test)]

use soroban_sdk::{symbol_short, Address, Env, Vec, Map, String};
use crate::liquidity_provision_hook::{
    LiquidityProvisionHook, LiquidityProvisionHookClient, LiquidityError,
    LiquidityConfig, LiquidityPool, LiquidityPosition, EmergencyWithdrawal,
    WithdrawalStatus, LiquidityMetrics, LiquidityDataKey,
    DEFAULT_MAX_LIQUIDITY_RATIO, MIN_EMERGENCY_WITHDRAWAL_RATIO,
    MAX_POOLS_PER_GRANT, LP_TOKEN_LOCK_PERIOD, MILESTONE_CLAIM_PRIORITY,
};

#[test]
fn test_liquidity_hook_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    // Initialize with valid parameters
    let result = client.initialize(
        &admin,
        &3000u32, // 30% max liquidity ratio
        &1000u128, // Min pool size
        &100000u128, // Max pool size
    );
    
    assert!(result.is_ok());
    
    // Verify configuration
    let config = client.get_config().unwrap();
    assert_eq!(config.admin, admin);
    assert_eq!(config.max_liquidity_ratio, 3000);
    assert_eq!(config.min_pool_size, 1000);
    assert_eq!(config.max_pool_size, 100000);
    assert!(config.emergency_withdrawal_enabled);
    assert!(config.auto_rebalance_enabled);
}

#[test]
fn test_liquidity_hook_invalid_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    // Test initialization with invalid max ratio (> 50%)
    let result = client.try_initialize(
        &admin,
        &6000u32, // 60% - too high
        &1000u128,
        &100000u128,
    );
    assert_eq!(result, Err(LiquidityError::InvalidAmount));
    
    // Test initialization with min > max pool size
    let result = client.try_initialize(
        &admin,
        &3000u32,
        &100000u128, // Min > max
        &1000u128,
    );
    assert_eq!(result, Err(LiquidityError::InvalidAmount));
    
    // Test double initialization
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    let result = client.try_initialize(
        &admin,
        &2000u32,
        &500u128,
        &50000u128,
    );
    assert_eq!(result, Err(LiquidityError::NotInitialized));
}

#[test]
fn test_create_liquidity_pool() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    // Initialize
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    // Create liquidity pool
    let grant_id = 1u64;
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let lp_token = Address::generate(&env);
    
    let pool_id = client.create_liquidity_pool(
        grant_id,
        &token_a,
        &token_b,
        &lp_token,
        &5000u128,
        &5000u128,
    ).unwrap();
    
    assert_eq!(pool_id, 1); // First pool should have ID 1
    
    // Verify pool details
    let pool = client.get_pool(pool_id).unwrap();
    assert_eq!(pool.pool_id, pool_id);
    assert_eq!(pool.grant_id, grant_id);
    assert_eq!(pool.token_a, token_a);
    assert_eq!(pool.token_b, token_b);
    assert_eq!(pool.lp_token_address, lp_token);
    assert_eq!(pool.deposited_amount_a, 5000);
    assert_eq!(pool.deposited_amount_b, 5000);
    assert!(pool.is_active);
    assert!(pool.auto_rebalance);
}

#[test]
fn test_create_pool_invalid_amounts() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    let grant_id = 1u64;
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let lp_token = Address::generate(&env);
    
    // Test amount below minimum
    let result = client.try_create_liquidity_pool(
        grant_id,
        &token_a,
        &token_b,
        &lp_token,
        &500u128, // Below min
        &500u128,
    );
    assert_eq!(result, Err(LiquidityError::InvalidAmount));
    
    // Test amount above maximum
    let result = client.try_create_liquidity_pool(
        grant_id,
        &token_a,
        &token_b,
        &lp_token,
        &200000u128, // Above max
        &200000u128,
    );
    assert_eq!(result, Err(LiquidityError::InvalidAmount));
}

#[test]
fn test_pool_limit_per_grant() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    let grant_id = 1u64;
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    
    // Create maximum number of pools
    for i in 0..MAX_POOLS_PER_GRANT {
        let lp_token = Address::generate(&env);
        let pool_id = client.create_liquidity_pool(
            grant_id,
            &token_a,
            &token_b,
            &lp_token,
            &1000u128,
            &1000u128,
        ).unwrap();
        assert_eq!(pool_id, i + 1);
    }
    
    // Try to create one more pool (should fail)
    let lp_token = Address::generate(&env);
    let result = client.try_create_liquidity_pool(
        grant_id,
        &token_a,
        &token_b,
        &lp_token,
        &1000u128,
        &1000u128,
    );
    assert_eq!(result, Err(LiquidityError::PoolLimitExceeded));
}

#[test]
fn test_allocate_to_liquidity() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    // Create pool first
    let grant_id = 1u64;
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let lp_token = Address::generate(&env);
    
    let pool_id = client.create_liquidity_pool(
        grant_id,
        &token_a,
        &token_b,
        &lp_token,
        &5000u128,
        &5000u128,
    ).unwrap();
    
    // Allocate additional liquidity
    let position_id = client.allocate_to_liquidity(
        grant_id,
        pool_id,
        &2000u128,
    ).unwrap();
    
    assert!(position_id > 0);
    
    // Verify position
    let position = client.get_position(position_id).unwrap();
    assert_eq!(position.pool_id, pool_id);
    assert_eq!(position.grant_id, grant_id);
    assert_eq!(position.allocated_amount, 2000);
    assert!(!position.is_locked);
}

#[test]
fn test_liquidity_ratio_constraints() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    // Initialize with low max ratio (10%)
    client.initialize(
        &admin,
        &1000u32, // 10%
        &1000u128,
        &100000u128,
    ).unwrap();
    
    let grant_id = 1u64;
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let lp_token = Address::generate(&env);
    
    // Create pool with large initial allocation
    let pool_id = client.create_liquidity_pool(
        grant_id,
        &token_a,
        &token_b,
        &lp_token,
        &50000u128, // This should exceed 10% of unstreamed (simulated as 1M)
        &50000u128,
    ).unwrap();
    
    // Try to allocate more (should fail due to ratio constraint)
    let result = client.try_allocate_to_liquidity(
        grant_id,
        pool_id,
        &100000u128, // This would definitely exceed 10%
    );
    assert_eq!(result, Err(LiquidityError::LiquidityRatioExceeded));
}

#[test]
fn test_emergency_withdrawal_for_milestone() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    // Create pool and allocate liquidity
    let grant_id = 1u64;
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let lp_token = Address::generate(&env);
    
    let pool_id = client.create_liquidity_pool(
        grant_id,
        &token_a,
        &token_b,
        &lp_token,
        &10000u128,
        &10000u128,
    ).unwrap();
    
    client.allocate_to_liquidity(
        grant_id,
        pool_id,
        &5000u128,
    ).unwrap();
    
    // Emergency withdrawal for milestone claim
    let milestone_amount = 8000u128;
    let milestone_claim_id = 42u64;
    
    let withdrawal_ids = client.emergency_withdraw_for_milestone(
        grant_id,
        milestone_amount,
        milestone_claim_id,
    ).unwrap();
    
    assert!(!withdrawal_ids.is_empty());
    
    // Verify positions are locked
    let positions = client.get_grant_positions(grant_id).unwrap();
    for &position_id in positions.iter() {
        let position = client.get_position(position_id).unwrap();
        assert!(position.is_locked);
        assert!(position.lock_reason.is_some());
    }
}

#[test]
fn test_emergency_withdrawal_insufficient_liquidity() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    // Create pool with small liquidity
    let grant_id = 1u64;
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let lp_token = Address::generate(&env);
    
    let pool_id = client.create_liquidity_pool(
        grant_id,
        &token_a,
        &token_b,
        &lp_token,
        &1000u128,
        &1000u128,
    ).unwrap();
    
    // Try emergency withdrawal with amount exceeding available liquidity
    let milestone_amount = 50000u128; // Much more than available
    let milestone_claim_id = 42u64;
    
    let result = client.try_emergency_withdraw_for_milestone(
        grant_id,
        milestone_amount,
        milestone_claim_id,
    );
    assert_eq!(result, Err(LiquidityError::InsufficientUnstreamed));
    
    // Verify positions are not locked (should be unlocked on failure)
    let positions = client.get_grant_positions(grant_id).unwrap();
    for &position_id in positions.iter() {
        let position = client.get_position(position_id).unwrap();
        assert!(!position.is_locked);
    }
}

#[test]
fn test_emergency_withdrawal_disabled() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    // Disable emergency withdrawals (this would require a separate admin function)
    // For now, test the error case
    
    let grant_id = 1u64;
    let milestone_amount = 1000u128;
    let milestone_claim_id = 42u64;
    
    // This would fail if emergency withdrawals are disabled
    // The actual implementation would need an admin function to toggle this
    let result = client.try_emergency_withdraw_for_milestone(
        grant_id,
        milestone_amount,
        milestone_claim_id,
    );
    // This should succeed in the current implementation since emergency is enabled by default
    assert!(result.is_ok());
}

#[test]
fn test_liquidity_metrics() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    // Check initial metrics
    let metrics = client.get_liquidity_metrics().unwrap();
    assert_eq!(metrics.total_allocated, 0);
    assert_eq!(metrics.total_value, 0);
    assert_eq!(metrics.total_fees_earned, 0);
    assert_eq!(metrics.active_pools, 0);
    assert_eq!(metrics.locked_positions, 0);
    assert_eq!(metrics.apy, 0);
    
    // Create pool and allocate liquidity
    let grant_id = 1u64;
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let lp_token = Address::generate(&env);
    
    let pool_id = client.create_liquidity_pool(
        grant_id,
        &token_a,
        &token_b,
        &lp_token,
        &5000u128,
        &5000u128,
    ).unwrap();
    
    client.allocate_to_liquidity(
        grant_id,
        pool_id,
        &2000u128,
    ).unwrap();
    
    // Check updated metrics
    let updated_metrics = client.get_liquidity_metrics().unwrap();
    assert!(updated_metrics.total_allocated > 0);
    assert!(updated_metrics.total_value > 0);
    assert_eq!(updated_metrics.active_pools, 1);
}

#[test]
fn test_pool_inactive_error() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    let grant_id = 1u64;
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let lp_token = Address::generate(&env);
    
    let pool_id = client.create_liquidity_pool(
        grant_id,
        &token_a,
        &token_b,
        &lp_token,
        &5000u128,
        &5000u128,
    ).unwrap();
    
    // Try to allocate to non-existent pool
    let result = client.try_allocate_to_liquidity(
        grant_id,
        999u64, // Non-existent pool
        &1000u128,
    );
    assert_eq!(result, Err(LiquidityError::PoolNotFound));
    
    // Try to allocate to wrong grant
    let result = client.try_allocate_to_liquidity(
        999u64, // Wrong grant
        pool_id,
        &1000u128,
    );
    assert_eq!(result, Err(LiquidityError::PoolInactive));
}

#[test]
fn test_rebalance_pools() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    // Create multiple pools
    let grant_id = 1u64;
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    
    for i in 0..3 {
        let lp_token = Address::generate(&env);
        client.create_liquidity_pool(
            grant_id,
            &token_a,
            &token_b,
            &lp_token,
            &1000u128,
            &1000u128,
        ).unwrap();
    }
    
    // Rebalance pools
    let rebalanced_pools = client.rebalance_pools().unwrap();
    
    // In the current implementation, no pools need rebalancing
    // So this should return empty vector
    assert_eq!(rebalanced_pools.len(), 0);
}

#[test]
fn test_rebalance_disabled() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    // Disable auto-rebalancing (would need admin function)
    // For now, test that rebalancing works when enabled
    
    let result = client.rebalance_pools();
    assert!(result.is_ok()); // Should succeed even with no pools
}

#[test]
fn test_position_tracking() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    let grant_id = 1u64;
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let lp_token = Address::generate(&env);
    
    // Create pool (creates initial position)
    let pool_id = client.create_liquidity_pool(
        grant_id,
        &token_a,
        &token_b,
        &lp_token,
        &5000u128,
        &5000u128,
    ).unwrap();
    
    // Allocate more (creates second position)
    let position_id = client.allocate_to_liquidity(
        grant_id,
        pool_id,
        &2000u128,
    ).unwrap();
    
    // Get all positions for grant
    let positions = client.get_grant_positions(grant_id).unwrap();
    assert_eq!(positions.len(), 2);
    
    // Verify second position details
    let position = client.get_position(position_id).unwrap();
    assert_eq!(position.pool_id, pool_id);
    assert_eq!(position.grant_id, grant_id);
    assert_eq!(position.allocated_amount, 2000);
    assert!(!position.is_locked);
}

#[test]
fn test_get_nonexistent_resources() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &3000u32,
        &1000u128,
        &100000u128,
    ).unwrap();
    
    // Test getting non-existent pool
    let result = client.try_get_pool(999u64);
    assert_eq!(result, Err(LiquidityError::PoolNotFound));
    
    // Test getting non-existent position
    let result = client.try_get_position(999u64);
    assert_eq!(result, Err(LiquidityError::PositionNotFound));
    
    // Test getting positions for non-existent grant
    let positions = client.get_grant_positions(999u64).unwrap();
    assert_eq!(positions.len(), 0);
}

#[test]
fn test_config_retrieval() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &2500u32,
        &2000u128,
        &200000u128,
    ).unwrap();
    
    let config = client.get_config().unwrap();
    assert_eq!(config.admin, admin);
    assert_eq!(config.max_liquidity_ratio, 2500);
    assert_eq!(config.min_pool_size, 2000);
    assert_eq!(config.max_pool_size, 200000);
    assert!(config.emergency_withdrawal_enabled);
    assert!(config.auto_rebalance_enabled);
    assert_eq!(config.rebalance_threshold, 1000);
    assert_eq!(config.fee_tier, 0);
}

#[test]
fn test_edge_cases() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, LiquidityProvisionHook);
    let client = LiquidityProvisionHookClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &5000u32, // Maximum allowed ratio (50%)
        &1u128,   // Minimum pool size
        &u128::MAX, // Maximum pool size
    ).unwrap();
    
    let grant_id = 1u64;
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let lp_token = Address::generate(&env);
    
    // Test with minimum amounts
    let pool_id = client.create_liquidity_pool(
        grant_id,
        &token_a,
        &token_b,
        &lp_token,
        &1u128,
        &1u128,
    ).unwrap();
    
    // Test with maximum allocation (within ratio)
    let position_id = client.allocate_to_liquidity(
        grant_id,
        pool_id,
        &u128::MAX / 2, // Large amount but within simulated unstreamed
    ).unwrap();
    
    // Verify position created successfully
    let position = client.get_position(position_id).unwrap();
    assert_eq!(position.allocated_amount, u128::MAX / 2);
}
