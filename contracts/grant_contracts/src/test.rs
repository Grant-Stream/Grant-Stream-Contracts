#![cfg(test)]

use super::*;
use soroban_sdk::{Address, Env, U256};

#[test]
fn test_basic_grant_functionality() {
    let env = Env::default();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    let total_amount = U256::from_u64(1000000);
    let duration = 86400; // 1 day

    client.initialize_grant(&recipient, &total_amount, &duration);
    
    let claimable = client.claimable_balance();
    assert_eq!(claimable, U256::from_u64(0));
    
    env.ledger().set_timestamp(env.ledger().timestamp() + 43200); // 12 hours later
    
    let claimable = client.claimable_balance();
    assert!(claimable > U256::from_u64(0));
}

#[test]
fn test_long_duration_simulation_10_years() {
    let env = Env::default();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    let total_amount = U256::from_u64(100000000); // 100M tokens
    let duration_10_years = 315360000; // 10 years in seconds
    
    let start_time = env.ledger().timestamp();
    let end_time = client.initialize_grant(&recipient, &total_amount, &duration_10_years);
    
    assert_eq!(end_time, start_time + duration_10_years);
    
    // Test at start - should be 0
    let claimable = client.claimable_balance();
    assert_eq!(claimable, U256::from_u64(0));
    
    // Test at year 5 (exactly halfway)
    let five_years_seconds = 157680000; // 5 years
    env.ledger().set_timestamp(start_time + five_years_seconds);
    
    let claimable_year_5 = client.claimable_balance();
    let expected_year_5 = total_amount * U256::from_u64(five_years_seconds) / U256::from_u64(duration_10_years);
    
    // Allow for small rounding differences (within 1 token)
    let diff = if claimable_year_5 > expected_year_5 {
        claimable_year_5 - expected_year_5
    } else {
        expected_year_5 - claimable_year_5
    };
    assert!(diff <= U256::from_u64(1), 
        "Claimable at year 5: {}, Expected: {}, Diff: {}", 
        claimable_year_5, expected_year_5, diff);
    
    // Verify it's approximately 50% of total
    let half_amount = total_amount / U256::from_u64(2);
    let diff_from_half = if claimable_year_5 > half_amount {
        claimable_year_5 - half_amount
    } else {
        half_amount - claimable_year_5
    };
    assert!(diff_from_half <= U256::from_u64(1), 
        "Should be approximately 50% at year 5");
    
    // Test at year 10 (end of grant)
    env.ledger().set_timestamp(end_time);
    
    let claimable_year_10 = client.claimable_balance();
    let expected_year_10 = total_amount; // Should be fully vested
    
    // Allow for small rounding differences
    let diff_end = if claimable_year_10 > expected_year_10 {
        claimable_year_10 - expected_year_10
    } else {
        expected_year_10 - claimable_year_10
    };
    assert!(diff_end <= U256::from_u64(1), 
        "Claimable at year 10: {}, Expected: {}, Diff: {}", 
        claimable_year_10, expected_year_10, diff_end);
    
    // Test beyond year 10 (should remain at total amount)
    env.ledger().set_timestamp(end_time + 1000000); // 1M seconds beyond
    
    let claimable_beyond = client.claimable_balance();
    assert_eq!(claimable_beyond, expected_year_10);
}

#[test]
fn test_claim_functionality_during_long_duration() {
    let env = Env::default();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    let total_amount = U256::from_u64(1000000);
    let duration_10_years = 315360000;
    
    let start_time = env.ledger().timestamp();
    client.initialize_grant(&recipient, &total_amount, &duration_10_years);
    
    // Advance to year 5 and claim
    let five_years_seconds = 157680000;
    env.ledger().set_timestamp(start_time + five_years_seconds);
    
    let claimable_before = client.claimable_balance();
    let claimed_amount = client.claim(&recipient);
    assert_eq!(claimed_amount, claimable_before);
    
    // After claiming, claimable should be 0
    let claimable_after = client.claimable_balance();
    assert_eq!(claimable_after, U256::from_u64(0));
    
    // Advance to year 10 and claim remaining
    env.ledger().set_timestamp(start_time + duration_10_years);
    
    let claimable_end = client.claimable_balance();
    let claimed_end = client.claim(&recipient);
    assert_eq!(claimed_end, claimable_end);
    
    // Total claimed should equal total amount
    let total_claimed = claimed_amount + claimed_end;
    let diff = if total_claimed > total_amount {
        total_claimed - total_amount
    } else {
        total_amount - total_claimed
    };
    assert!(diff <= U256::from_u64(1), 
        "Total claimed: {}, Expected: {}, Diff: {}", 
        total_claimed, total_amount, diff);
}

#[test]
fn test_timestamp_math_no_overflow() {
    let env = Env::default();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    let total_amount = U256::from_u64(u64::MAX / 2); // Large amount
    let duration_10_years = 315360000;
    
    // Start at a high timestamp to test overflow conditions
    let high_timestamp = u64::MAX - duration_10_years - 1000000;
    env.ledger().set_timestamp(high_timestamp);
    
    let end_time = client.initialize_grant(&recipient, &total_amount, &duration_10_years);
    
    // Verify end_time doesn't overflow
    assert!(end_time > high_timestamp);
    assert!(end_time <= u64::MAX);
    
    // Test calculations at various points
    env.ledger().set_timestamp(high_timestamp + duration_10_years / 2);
    let claimable_mid = client.claimable_balance();
    assert!(claimable_mid > U256::from_u64(0));
    
    env.ledger().set_timestamp(end_time);
    let claimable_end = client.claimable_balance();
    assert!(claimable_end > U256::from_u64(0));
}

#[test]
fn test_grant_info_function() {
    let env = Env::default();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let recipient = Address::generate(&env);
    let total_amount = U256::from_u64(5000000);
    let duration = 86400 * 365; // 1 year
    
    let start_time = env.ledger().timestamp();
    let end_time = client.initialize_grant(&recipient, &total_amount, &duration);
    
    let (stored_amount, stored_start, stored_end, claimed) = client.get_grant_info();
    
    assert_eq!(stored_amount, total_amount);
    assert_eq!(stored_start, start_time);
    assert_eq!(stored_end, end_time);
    assert_eq!(claimed, U256::from_u64(0));
}
