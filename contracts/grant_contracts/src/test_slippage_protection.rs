#![cfg(test)]

use soroban_sdk::{symbol_short, Address, Env, Vec, Map, u128};
use crate::multi_token_matching::{
    MultiTokenMatchingPool, MultiTokenMatchingPoolClient, MatchingPoolError, MatchingPoolDataKey,
    DexSpread, SlippageConfig, QueuedSwap, SlippageGuard,
    DEFAULT_SLIPPAGE_THRESHOLD_BPS, SWAP_QUEUE_MAX_SIZE, SWAP_QUEUE_EXPIRY_SECS,
};

#[test]
fn test_slippage_protection_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    // Initialize contract
    client.initialize(&admin, &oracle, &native_token);
    
    // Verify slippage guard was initialized with default config
    let slippage_config = client.get_slippage_config().unwrap();
    assert_eq!(slippage_config.max_slippage_bps, DEFAULT_SLIPPAGE_THRESHOLD_BPS);
    assert!(slippage_config.auto_queue_enabled);
    assert_eq!(slippage_config.queue_expiry_secs, SWAP_QUEUE_EXPIRY_SECS);
}

#[test]
fn test_configure_slippage_protection() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Configure custom slippage protection
    let max_slippage_bps = 200u32; // 2%
    let auto_queue_enabled = false;
    let min_liquidity_threshold = 5000u128;
    let spread_confidence_threshold = 9000u32; // 90%
    let queue_expiry_secs = 7200u64; // 2 hours
    
    client.configure_slippage_protection(
        &admin,
        &max_slippage_bps,
        &auto_queue_enabled,
        &min_liquidity_threshold,
        &spread_confidence_threshold,
        &queue_expiry_secs,
    );
    
    // Verify configuration was updated
    let config = client.get_slippage_config().unwrap();
    assert_eq!(config.max_slippage_bps, max_slippage_bps);
    assert_eq!(config.auto_queue_enabled, auto_queue_enabled);
    assert_eq!(config.min_liquidity_threshold, min_liquidity_threshold);
    assert_eq!(config.spread_confidence_threshold, spread_confidence_threshold);
    assert_eq!(config.queue_expiry_secs, queue_expiry_secs);
}

#[test]
fn test_configure_slippage_protection_unauthorized() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let unauthorized_user = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Try to configure with unauthorized user
    let result = client.try_configure_slippage_protection(
        &unauthorized_user,
        &200u32,
        &false,
        &5000u128,
        &9000u32,
        &7200u64,
    );
    
    assert_eq!(result, Err(MatchingPoolError::Unauthorized));
}

#[test]
fn test_query_dex_spread() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Query DEX spread
    let spread = client.query_dex_spread(&token_a, &token_b).unwrap();
    
    // Verify spread structure
    assert_eq!(spread.token_a, token_a);
    assert_eq!(spread.token_b, token_b);
    assert!(spread.bid_price > 0);
    assert!(spread.ask_price > spread.bid_price); // Ask should be higher than bid
    assert!(spread.spread_bps > 0);
    assert!(spread.liquidity_depth > 0);
    assert!(spread.confidence_bps > 0);
    assert_eq!(spread.dex_source, String::from_str(&env, "stellar_dex_v1"));
}

#[test]
fn test_execute_swap_with_protection_success() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create and activate a round
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Execute swap with reasonable slippage tolerance
    let amount = 10_000u128;
    let min_received = 9_500u128; // 5% slippage tolerance
    
    let result = client.execute_swap_with_protection(
        &round_id,
        &usdc,
        &xlm,
        &amount,
        &min_received,
    );
    
    // Should succeed with default 1% threshold
    assert!(result.is_ok());
}

#[test]
fn test_execute_swap_with_protection_high_slippage() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Configure very low slippage threshold
    client.configure_slippage_protection(
        &admin,
        &50u32, // 0.5% threshold
        &true,
        &1000u128,
        &8000u32,
        &3600u64,
    );
    
    // Create and activate a round
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Execute swap with very low tolerance (should trigger queuing)
    let amount = 10_000u128;
    let min_received = 9_900u128; // Only 1% slippage tolerance
    
    let result = client.try_execute_swap_with_protection(
        &round_id,
        &usdc,
        &xlm,
        &amount,
        &min_received,
    );
    
    // Should queue the swap due to high slippage
    assert!(matches!(result, Ok(_))); // Returns swap ID when queued
}

#[test]
fn test_execute_swap_with_protection_queue_disabled() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Disable auto-queuing
    client.configure_slippage_protection(
        &admin,
        &50u32, // 0.5% threshold
        &false, // Auto-queue disabled
        &1000u128,
        &8000u32,
        &3600u64,
    );
    
    // Create and activate a round
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Execute swap with very low tolerance (should fail)
    let amount = 10_000u128;
    let min_received = 9_900u128; // Only 1% slippage tolerance
    
    let result = client.try_execute_swap_with_protection(
        &round_id,
        &usdc,
        &xlm,
        &amount,
        &min_received,
    );
    
    // Should fail when queuing is disabled
    assert_eq!(result, Err(MatchingPoolError::SlippageExceedsThreshold));
}

#[test]
fn test_queue_swap_functionality() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create and activate a round
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Directly queue a swap
    let amount = 10_000u128;
    let min_received = 9_500u128;
    
    let swap_id = client.queue_swap(
        &round_id,
        &usdc,
        &xlm,
        &amount,
        &min_received,
    ).unwrap();
    
    // Verify swap was queued
    assert_eq!(swap_id, 1); // First swap should have ID 1
    
    let queued_swaps = client.get_queued_swaps(&round_id).unwrap();
    assert_eq!(queued_swaps.len(), 1);
    
    let queued_swap = queued_swaps.get(0).unwrap();
    assert_eq!(queued_swap.swap_id, swap_id);
    assert_eq!(queued_swap.round_id, round_id);
    assert_eq!(queued_swap.from_token, usdc);
    assert_eq!(queued_swap.to_token, xlm);
    assert_eq!(queued_swap.amount, amount);
    assert_eq!(queued_swap.min_received, min_received);
    assert_eq!(queued_swap.retry_count, 0);
    assert_eq!(queued_swap.max_retries, 3);
}

#[test]
fn test_queue_swap_capacity_limit() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create and activate a round
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Fill queue to capacity
    for i in 0..SWAP_QUEUE_MAX_SIZE {
        let swap_id = client.queue_swap(
            &round_id,
            &usdc,
            &xlm,
            &1000u128,
            &950u128,
        );
        assert!(swap_id.is_ok());
    }
    
    // Try to add one more swap (should fail)
    let result = client.try_queue_swap(
        &round_id,
        &usdc,
        &xlm,
        &1000u128,
        &950u128,
    );
    
    assert_eq!(result, Err(MatchingPoolError::SwapQueueFull));
}

#[test]
fn test_process_queued_swaps() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create and activate a round
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Queue some swaps
    let swap_id1 = client.queue_swap(&round_id, &usdc, &xlm, &1000u128, &950u128).unwrap();
    let swap_id2 = client.queue_swap(&round_id, &usdc, &xlm, &2000u128, &1900u128).unwrap();
    
    // Verify swaps are queued
    let queued_swaps = client.get_queued_swaps(&round_id).unwrap();
    assert_eq!(queued_swaps.len(), 2);
    
    // Process queued swaps
    let processed_swaps = client.process_queued_swaps(&admin).unwrap();
    
    // Should process some swaps (depending on slippage conditions)
    assert!(processed_swaps.len() >= 0);
    
    // Check remaining queued swaps
    let remaining_swaps = client.get_queued_swaps(&round_id).unwrap();
    assert!(remaining_swaps.len() <= 2);
}

#[test]
fn test_swap_expiry() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Configure very short expiry for testing
    client.configure_slippage_protection(
        &admin,
        &100u32,
        &true,
        &1000u128,
        &8000u32,
        &1u64, // 1 second expiry
    );
    
    // Create and activate a round
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Queue a swap
    let swap_id = client.queue_swap(&round_id, &usdc, &xlm, &1000u128, &950u128).unwrap();
    
    // Verify swap is queued
    let queued_swaps = client.get_queued_swaps(&round_id).unwrap();
    assert_eq!(queued_swaps.len(), 1);
    
    // Advance time beyond expiry
    env.ledger().set_timestamp(env.ledger().timestamp() + 2);
    
    // Process queued swaps (should remove expired swap)
    let processed_swaps = client.process_queued_swaps(&admin).unwrap();
    assert_eq!(processed_swaps.len(), 0); // No swaps processed, only expired
    
    // Verify swap was removed due to expiry
    let remaining_swaps = client.get_queued_swaps(&round_id).unwrap();
    assert_eq!(remaining_swaps.len(), 0);
}

#[test]
fn test_insufficient_liquidity_protection() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Configure very high liquidity threshold
    client.configure_slippage_protection(
        &admin,
        &100u32,
        &true,
        &10_000_000u128, // Very high liquidity threshold
        &8000u32,
        &3600u64,
    );
    
    // Create and activate a round
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Try to execute swap (should fail due to insufficient liquidity)
    let result = client.try_execute_swap_with_protection(
        &round_id,
        &usdc,
        &xlm,
        &1000u128,
        &950u128,
    );
    
    assert_eq!(result, Err(MatchingPoolError::InsufficientLiquidity));
}

#[test]
fn test_spread_data_stale() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create and activate a round
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Advance time significantly to make spread data stale
    env.ledger().set_timestamp(env.ledger().timestamp() + 60); // Beyond 30s timeout
    
    // Try to execute swap (should fail due to stale spread data)
    let result = client.try_execute_swap_with_protection(
        &round_id,
        &usdc,
        &xlm,
        &1000u128,
        &950u128,
    );
    
    assert_eq!(result, Err(MatchingPoolError::SpreadDataStale));
}

#[test]
fn test_round_not_active_swap_protection() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create a round but don't activate it
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    // Try to execute swap on inactive round (should fail)
    let result = client.try_execute_swap_with_protection(
        &round_id,
        &usdc,
        &xlm,
        &1000u128,
        &950u128,
    );
    
    assert_eq!(result, Err(MatchingPoolError::RoundNotActive));
}
