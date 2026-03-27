#![cfg(test)]

use soroban_sdk::{symbol_short, Address, Env, Vec, Map, u128};
use crate::multi_token_matching::{
    MultiTokenMatchingPool, MultiTokenMatchingPoolClient, MatchingPoolError, MatchingPoolDataKey,
    TokenPrice, Donation, ProjectDonations, MatchingRound, PriceFeedData,
    PRICE_FEED_STALENESS_THRESHOLD, VOLATILITY_THRESHOLD_BPS, MATCHING_PRECISION, QUADRATIC_PRECISION
};

#[test]
fn test_matching_pool_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    // Test successful initialization
    client.initialize(&admin, &oracle, &native_token);
    
    // Verify admin is set
    let stored_admin = env.storage().instance().get(&MatchingPoolDataKey::Admin).unwrap();
    assert_eq!(stored_admin, admin);
    
    // Verify oracle is set
    let price_feed = client.get_price_feed().unwrap();
    assert_eq!(price_feed.oracle_address, oracle);
    
    // Verify native token is set
    let stored_native = env.storage().instance().get(&MatchingPoolDataKey::NativeToken).unwrap();
    assert_eq!(stored_native, native_token);
    
    // Verify initial state
    let next_round_id = env.storage().instance().get(&MatchingPoolDataKey::NextRoundId).unwrap();
    assert_eq!(next_round_id, 1);
    
    let total_pool = client.get_total_matching_pool().unwrap();
    assert_eq!(total_pool, 0);
}

#[test]
fn test_matching_pool_double_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    // First initialization should succeed
    client.initialize(&admin, &oracle, &native_token);
    
    // Second initialization should fail
    let result = client.try_initialize(&admin, &oracle, &native_token);
    assert_eq!(result, Err(MatchingPoolError::NotInitialized));
}

#[test]
fn test_create_matching_round() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400 * 7; // 7 days
    let matching_pool = 1_000_000u128; // 1M tokens
    let supported_tokens = Vec::from_array(&env, [usdc, xlm]);
    let min_donation = 1000u128;
    let max_donation = 100_000u128;
    let quadratic_coefficient = 1_000_000u128; // 1.0 coefficient
    
    // Create round
    let round_id = client.create_round(
        &admin,
        &matching_pool,
        &start_time,
        &end_time,
        &supported_tokens,
        &min_donation,
        &max_donation,
        &quadratic_coefficient,
    );
    
    assert!(round_id.is_ok());
    assert_eq!(round_id.unwrap(), 1); // First round should have ID 1
    
    // Verify round was created
    let round = client.get_round(&1).unwrap();
    assert_eq!(round.round_id, 1);
    assert_eq!(round.matching_pool_amount, matching_pool);
    assert_eq!(round.start_time, start_time);
    assert_eq!(round.end_time, end_time);
    assert_eq!(round.native_token_address, native_token);
    assert_eq!(round.supported_tokens.len(), 2);
    assert!(!round.is_active);
    assert!(!round.matching_calculated);
    
    // Verify total matching pool was updated
    let total_pool = client.get_total_matching_pool().unwrap();
    assert_eq!(total_pool, matching_pool);
}

#[test]
fn test_create_round_invalid_config() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    let start_time = env.ledger().timestamp();
    let end_time = start_time - 1000; // End time before start time (invalid)
    
    // Test creating round with invalid config
    let result = client.try_create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::new(&env),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    );
    
    assert_eq!(result, Err(MatchingPoolError::InvalidRoundConfig));
}

#[test]
fn test_activate_round() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create round
    let start_time = env.ledger().timestamp() + 3600; // Start in 1 hour
    let end_time = start_time + 86400 * 7; // 7 days
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::new(&env),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    // Activate round
    client.activate_round(&admin, &round_id);
    
    // Verify round is active
    let round = client.get_round(&round_id).unwrap();
    assert!(round.is_active);
    
    // Verify active round is set
    let active_round_id = env.storage().instance().get(&MatchingPoolDataKey::ActiveRound).unwrap();
    assert_eq!(active_round_id, round_id);
}

#[test]
fn test_make_donation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create and activate round
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400; // 1 day
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
    
    // Mock token price
    let token_price = TokenPrice {
        token_address: usdc,
        price_in_native: 1_000_000u128, // 1 USDC = 1 native token (with precision)
        timestamp: env.ledger().timestamp(),
        confidence_bps: 9500, // 95% confidence
        volume_24h: 1_000_000u128,
    };
    
    let mut price_feed = PriceFeedData {
        token_prices: Map::new(&env),
        last_updated: env.ledger().timestamp(),
        oracle_address: oracle,
    };
    price_feed.token_prices.set(usdc, token_price);
    env.storage().instance().set(&MatchingPoolDataKey::PriceFeed, &price_feed);
    
    // Make donation
    let project_id = 123u64;
    let donation_amount = 10_000u128; // 10,000 USDC
    
    client.donate(&donor, &round_id, &project_id, &usdc, &donation_amount);
    
    // Verify donation was recorded
    let project_donations = client.get_project_donations(&round_id, &project_id).unwrap();
    assert_eq!(project_donations.project_id, project_id);
    assert_eq!(project_donations.total_normalized_value, 10_000_000_000u128); // 10,000 * 1,000,000 precision
    assert_eq!(project_donations.unique_donors, 1);
    assert_eq!(project_donations.donations.len(), 1);
    
    // Verify user donations
    let user_donations = client.get_user_donations(&donor, &round_id);
    assert_eq!(user_donations.len(), 1);
    assert_eq!(user_donations.get(0).unwrap().amount, donation_amount);
}

#[test]
fn test_donation_invalid_amount() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create and activate round
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128, // Min donation
        &100_000u128, // Max donation
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Mock token price
    let token_price = TokenPrice {
        token_address: usdc,
        price_in_native: 1_000_000u128,
        timestamp: env.ledger().timestamp(),
        confidence_bps: 9500,
        volume_24h: 1_000_000u128,
    };
    
    let mut price_feed = PriceFeedData {
        token_prices: Map::new(&env),
        last_updated: env.ledger().timestamp(),
        oracle_address: oracle,
    };
    price_feed.token_prices.set(usdc, token_price);
    env.storage().instance().set(&MatchingPoolDataKey::PriceFeed, &price_feed);
    
    // Try donation with amount below minimum
    let result = client.try_donate(&donor, &round_id, &123u64, &usdc, &500u128);
    assert_eq!(result, Err(MatchingPoolError::InvalidAmount));
    
    // Try donation with amount above maximum
    let result = client.try_donate(&donor, &round_id, &123u64, &usdc, &200_000u128);
    assert_eq!(result, Err(MatchingPoolError::InvalidAmount));
}

#[test]
fn test_donation_invalid_token() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let unsupported_token = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create and activate round with only USDC supported
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]), // Only USDC supported
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Try donation with unsupported token
    let result = client.try_donate(&donor, &round_id, &123u64, &unsupported_token, &10_000u128);
    assert_eq!(result, Err(MatchingPoolError::InvalidToken));
}

#[test]
fn test_price_feed_stale() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create and activate round
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
    
    // Mock stale token price (older than threshold)
    let stale_timestamp = env.ledger().timestamp() - PRICE_FEED_STALENESS_THRESHOLD - 100;
    let token_price = TokenPrice {
        token_address: usdc,
        price_in_native: 1_000_000u128,
        timestamp: stale_timestamp,
        confidence_bps: 9500,
        volume_24h: 1_000_000u128,
    };
    
    let mut price_feed = PriceFeedData {
        token_prices: Map::new(&env),
        last_updated: stale_timestamp,
        oracle_address: oracle,
    };
    price_feed.token_prices.set(usdc, token_price);
    env.storage().instance().set(&MatchingPoolDataKey::PriceFeed, &price_feed);
    
    // Try donation with stale price
    let result = client.try_donate(&Address::generate(&env), &round_id, &123u64, &usdc, &10_000u128);
    assert_eq!(result, Err(MatchingPoolError::PriceFeedStale));
}

#[test]
fn test_high_volatility_protection() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create and activate round
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
    
    // Mock token price with low confidence (high volatility)
    let token_price = TokenPrice {
        token_address: usdc,
        price_in_native: 1_000_000u128,
        timestamp: env.ledger().timestamp(),
        confidence_bps: 8000, // 80% confidence (below 90% threshold)
        volume_24h: 1_000_000u128,
    };
    
    let mut price_feed = PriceFeedData {
        token_prices: Map::new(&env),
        last_updated: env.ledger().timestamp(),
        oracle_address: oracle,
    };
    price_feed.token_prices.set(usdc, token_price);
    env.storage().instance().set(&MatchingPoolDataKey::PriceFeed, &price_feed);
    
    // Try donation with high volatility
    let result = client.try_donate(&Address::generate(&env), &round_id, &123u64, &usdc, &10_000u128);
    assert_eq!(result, Err(MatchingPoolError::HighVolatility));
}

#[test]
fn test_over_allocation_protection() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create round with small matching pool
    let start_time = env.ledger().timestamp();
    let end_time = start_time + 86400;
    let matching_pool = 100_000u128; // Small matching pool
    let round_id = client.create_round(
        &admin,
        &matching_pool,
        &start_time,
        &end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Mock token price
    let token_price = TokenPrice {
        token_address: usdc,
        price_in_native: 1_000_000u128,
        timestamp: env.ledger().timestamp(),
        confidence_bps: 9500,
        volume_24h: 1_000_000u128,
    };
    
    let mut price_feed = PriceFeedData {
        token_prices: Map::new(&env),
        last_updated: env.ledger().timestamp(),
        oracle_address: oracle,
    };
    price_feed.token_prices.set(usdc, token_price);
    env.storage().instance().set(&MatchingPoolDataKey::PriceFeed, &price_feed);
    
    // Make large donation that would exceed 2x matching pool
    let large_donation = 300_000u128; // Would exceed 2x matching pool when normalized
    let result = client.try_donate(&donor, &round_id, &123u64, &usdc, &large_donation);
    assert_eq!(result, Err(MatchingPoolError::OverAllocationRisk));
}

#[test]
fn test_quadratic_matching_calculation() {
    let env = Env::default();
    
    // Test integer square root function
    let test_values = vec![
        (0u128, 0u128),
        (1u128, 1u128),
        (4u128, 2u128),
        (9u128, 3u128),
        (16u128, 4u128),
        (25u128, 5u128),
        (100u128, 10u128),
        (10000u128, 100u128),
        (1000000u128, 1000u128),
    ];
    
    for (input, expected) in test_values {
        let result = MultiTokenMatchingPool::integer_square_root(input).unwrap();
        assert_eq!(result, expected, "Failed for input: {}", input);
    }
}

#[test]
fn test_multi_token_normalization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Mock prices: 1 USDC = 1 native, 1 XLM = 0.1 native
    let usdc_price = TokenPrice {
        token_address: usdc,
        price_in_native: 1_000_000u128, // 1:1 with precision
        timestamp: env.ledger().timestamp(),
        confidence_bps: 9500,
        volume_24h: 1_000_000u128,
    };
    
    let xlm_price = TokenPrice {
        token_address: xlm,
        price_in_native: 100_000u128, // 0.1:1 with precision
        timestamp: env.ledger().timestamp(),
        confidence_bps: 9500,
        volume_24h: 1_000_000u128,
    };
    
    let mut price_feed = PriceFeedData {
        token_prices: Map::new(&env),
        last_updated: env.ledger().timestamp(),
        oracle_address: oracle,
    };
    price_feed.token_prices.set(usdc, usdc_price);
    price_feed.token_prices.set(xlm, xlm_price);
    env.storage().instance().set(&MatchingPoolDataKey::PriceFeed, &price_feed);
    
    // Test USDC normalization
    let usdc_amount = 10_000u128;
    let usdc_normalized = client.normalize_token_value(&usdc, &usdc_amount).unwrap();
    assert_eq!(usdc_normalized, 10_000_000_000_000u128); // 10,000 * 1,000,000 precision
    
    // Test XLM normalization
    let xlm_amount = 10_000u128;
    let xlm_normalized = client.normalize_token_value(&xlm, &xlm_amount).unwrap();
    assert_eq!(xlm_normalized, 1_000_000_000_000u128); // 10,000 * 100,000 precision
    
    // Verify XLM is worth 1/10 of USDC
    assert_eq!(xlm_normalized * 10, usdc_normalized);
}

#[test]
fn test_round_edge_cases() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Test getting non-existent round
    let result = client.try_get_round(&999);
    assert_eq!(result, Err(MatchingPoolError::RoundNotFound));
    
    // Test getting project donations for non-existent project
    let result = client.try_get_project_donations(&1, &999);
    assert_eq!(result, Err(MatchingPoolError::RoundNotFound));
    
    // Test getting user donations for user with no donations
    let user_donations = client.get_user_donations(&Address::generate(&env), &1);
    assert_eq!(user_donations.len(), 0);
    
    // Test getting token price for non-existent token
    let result = client.try_get_token_price(&Address::generate(&env));
    assert_eq!(result, Err(MatchingPoolError::InvalidToken));
}

#[test]
fn test_donation_period_validation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let donor = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create round that hasn't started yet
    let future_start_time = env.ledger().timestamp() + 3600; // Start in 1 hour
    let future_end_time = future_start_time + 86400;
    let round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &future_start_time,
        &future_end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &round_id);
    
    // Mock token price
    let token_price = TokenPrice {
        token_address: usdc,
        price_in_native: 1_000_000u128,
        timestamp: env.ledger().timestamp(),
        confidence_bps: 9500,
        volume_24h: 1_000_000u128,
    };
    
    let mut price_feed = PriceFeedData {
        token_prices: Map::new(&env),
        last_updated: env.ledger().timestamp(),
        oracle_address: oracle,
    };
    price_feed.token_prices.set(usdc, token_price);
    env.storage().instance().set(&MatchingPoolDataKey::PriceFeed, &price_feed);
    
    // Try donation before round starts
    let result = client.try_donate(&donor, &round_id, &123u64, &usdc, &10_000u128);
    assert_eq!(result, Err(MatchingPoolError::DonationPeriodEnded));
    
    // Create round that has already ended
    let past_start_time = env.ledger().timestamp() - 86400 * 2; // Started 2 days ago
    let past_end_time = past_start_time + 86400; // Ended 1 day ago
    let past_round_id = client.create_round(
        &admin,
        &1_000_000u128,
        &past_start_time,
        &past_end_time,
        &Vec::from_array(&env, [usdc]),
        &1000u128,
        &100_000u128,
        &1_000_000u128,
    ).unwrap();
    
    client.activate_round(&admin, &past_round_id);
    
    // Try donation after round ends
    let result = client.try_donate(&donor, &past_round_id, &123u64, &usdc, &10_000u128);
    assert_eq!(result, Err(MatchingPoolError::DonationPeriodEnded));
}

#[test]
fn test_multiple_donations_same_project() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let donor1 = Address::generate(&env);
    let donor2 = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let usdc = Address::generate(&env);
    
    let contract_id = env.register_contract(None, MultiTokenMatchingPool);
    let client = MultiTokenMatchingPoolClient::new(&env, &contract_id);
    
    client.initialize(&admin, &oracle, &native_token);
    
    // Create and activate round
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
    
    // Mock token price
    let token_price = TokenPrice {
        token_address: usdc,
        price_in_native: 1_000_000u128,
        timestamp: env.ledger().timestamp(),
        confidence_bps: 9500,
        volume_24h: 1_000_000u128,
    };
    
    let mut price_feed = PriceFeedData {
        token_prices: Map::new(&env),
        last_updated: env.ledger().timestamp(),
        oracle_address: oracle,
    };
    price_feed.token_prices.set(usdc, token_price);
    env.storage().instance().set(&MatchingPoolDataKey::PriceFeed, &price_feed);
    
    let project_id = 123u64;
    
    // First donation
    client.donate(&donor1, &round_id, &project_id, &usdc, &10_000u128);
    
    // Second donation from different donor
    client.donate(&donor2, &round_id, &project_id, &usdc, &5_000u128);
    
    // Verify project donations
    let project_donations = client.get_project_donations(&round_id, &project_id).unwrap();
    assert_eq!(project_donations.unique_donors, 2);
    assert_eq!(project_donations.donations.len(), 2);
    assert_eq!(project_donations.total_normalized_value, 15_000_000_000_000u128); // 15,000 normalized
    
    // Verify individual donor records
    let donor1_donations = client.get_user_donations(&donor1, &round_id);
    assert_eq!(donor1_donations.len(), 1);
    
    let donor2_donations = client.get_user_donations(&donor2, &round_id);
    assert_eq!(donor2_donations.len(), 1);
}
