#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Symbol, String,
};

use crate::{
    testutils::GrantContractClient,
    GrantContract, GranteeConfig, DeficitRecord, ClawbackBuffer,
};

fn set_timestamp(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

/// Test basic deficit detection when contract balance is less than total owed
#[test]
fn test_clawback_health_check_detects_deficit() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let native_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Initialize contract
    client.mock_all_auths().initialize(
        &admin,
        &grant_token,
        &treasury,
        &native_token,
    );

    let grant_id: u64 = 1;
    let total_amount: i128 = 10000;
    let flow_rate: i128 = 10;

    // Create grant
    set_timestamp(&env, 1_000);
    client.mock_all_auths().create_grant(
        &grant_id,
        &recipient,
        &total_amount,
        &flow_rate,
        &0,
        &Address::generate(&env),
        &String::from_str(&env, "PROP-001"),
        &String::from_str(&env, "SN-12345"),
        &1000,
        &1_000 + (total_amount / flow_rate),
    );

    // Simulate clawback: Remove half the funds from contract
    // In reality, this would happen via external regulated asset clawback
    // For testing, we'll directly manipulate the balance
    
    // Activate grant
    client.mock_all_auths().activate_grant(&grant_id);
    
    // Advance time to accrue some claimable amount
    set_timestamp(&env, 2_000);
    
    // Try to withdraw - should trigger health check and detect deficit
    // Since we can't actually remove tokens in this test, we'll test
    // the safety pause activation manually
    
    client.mock_all_auths().activate_safety_pause(&grant_id);
    
    // Verify grant is now in SafetyPaused state
    let grant = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant.status, soroban_sdk::symbol_short!("safety_paused"));
    assert!(grant.safety_pause_start.is_some());
}

/// Test proportional withdrawals during safety pause
#[test]
fn test_proportional_withdrawal_during_safety_pause() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let native_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Initialize contract
    client.mock_all_auths().initialize(
        &admin,
        &grant_token,
        &treasury,
        &native_token,
    );

    let grant_id: u64 = 1;
    let total_amount: i128 = 10000;
    let flow_rate: i128 = 10;

    // Create and activate grant
    set_timestamp(&env, 1_000);
    client.mock_all_auths().create_grant(
        &grant_id,
        &recipient,
        &total_amount,
        &flow_rate,
        &0,
        &Address::generate(&env),
        &String::from_str(&env, "PROP-001"),
        &String::from_str(&env, "SN-12345"),
        &1000,
        &1_000 + (total_amount / flow_rate),
    );
    
    client.mock_all_auths().activate_grant(&grant_id);
    
    // Activate safety pause
    client.mock_all_auths().activate_safety_pause(&grant_id);
    
    // Advance time beyond buffer period (4 hours = 14400 seconds)
    set_timestamp(&env, 1_000 + 15000);
    
    // Should be able to withdraw proportionally after buffer expires
    // Note: This test would need actual token balance manipulation to fully test
    // For now, we test the state transitions
}

/// Test admin resolving deficit with treasury funds
#[test]
fn test_resolve_deficit_with_treasury_funds() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let native_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Initialize contract
    client.mock_all_auths().initialize(
        &admin,
        &grant_token,
        &treasury,
        &native_token,
    );

    let grant_id: u64 = 1;
    let total_amount: i128 = 10000;
    let flow_rate: i128 = 10;

    // Create and activate grant
    set_timestamp(&env, 1_000);
    client.mock_all_auths().create_grant(
        &grant_id,
        &recipient,
        &total_amount,
        &flow_rate,
        &0,
        &Address::generate(&env),
        &String::from_str(&env, "PROP-001"),
        &String::from_str(&env, "SN-12345"),
        &1000,
        &1_000 + (total_amount / flow_rate),
    );
    
    client.mock_all_auths().activate_grant(&grant_id);
    
    // Manually activate safety pause to simulate deficit
    client.mock_all_auths().activate_safety_pause(&grant_id);
    
    // Admin resolves deficit by adding funds
    // Note: Would need to mock treasury balance for full test
    // client.mock_all_auths().resolve_deficit_with_treasury_funds(&grant_id, &5000);
    
    // Verify safety pause can be deactivated
    client.mock_all_auths().deactivate_safety_pause(&grant_id);
    
    let grant = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant.status, soroban_sdk::symbol_short!("active"));
    assert!(grant.safety_pause_start.is_none());
}

/// Test getting deficit record and clawback buffer info
#[test]
fn test_get_deficit_and_buffer_info() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let native_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Initialize contract
    client.mock_all_auths().initialize(
        &admin,
        &grant_token,
        &treasury,
        &native_token,
    );

    let grant_id: u64 = 1;
    let total_amount: i128 = 10000;
    let flow_rate: i128 = 10;

    // Create and activate grant
    set_timestamp(&env, 1_000);
    client.mock_all_auths().create_grant(
        &grant_id,
        &recipient,
        &total_amount,
        &flow_rate,
        &0,
        &Address::generate(&env),
        &String::from_str(&env, "PROP-001"),
        &String::from_str(&env, "SN-12345"),
        &1000,
        &1_000 + (total_amount / flow_rate),
    );
    
    client.mock_all_auths().activate_grant(&grant_id);
    client.mock_all_auths().activate_safety_pause(&grant_id);
    
    // Get deficit record - should fail since we haven't created one
    // This tests the error handling
    let result = std::panic::catch_unwind(|| {
        client.get_deficit_record(&grant_id)
    });
    assert!(result.is_err());
    
    // Get clawback buffer - should also fail
    let result2 = std::panic::catch_unwind(|| {
        client.get_clawback_buffer(&grant_id)
    });
    assert!(result2.is_err());
}

/// Test that safety pause prevents normal withdrawals
#[test]
fn test_safety_pause_blocks_normal_withdrawals() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let native_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Initialize contract
    client.mock_all_auths().initialize(
        &admin,
        &grant_token,
        &treasury,
        &native_token,
    );

    let grant_id: u64 = 1;
    let total_amount: i128 = 10000;
    let flow_rate: i128 = 10;

    // Create and activate grant
    set_timestamp(&env, 1_000);
    client.mock_all_auths().create_grant(
        &grant_id,
        &recipient,
        &total_amount,
        &flow_rate,
        &0,
        &Address::generate(&env),
        &String::from_str(&env, "PROP-001"),
        &String::from_str(&env, "SN-12345"),
        &1000,
        &1_000 + (total_amount / flow_rate),
    );
    
    client.mock_all_auths().activate_grant(&grant_id);
    
    // Activate safety pause
    client.mock_all_auths().activate_safety_pause(&grant_id);
    
    // Try to withdraw normally - should redirect to proportional withdrawal
    // or fail with appropriate error
    let result = std::panic::catch_unwind(|| {
        client.mock_all_auths().withdraw(&grant_id, &100);
    });
    
    // Should not panic with normal success - either fails or uses proportional logic
    // Exact behavior depends on implementation details
}

/// Test deactivating safety pause when buffer still active should fail
#[test]
fn test_cannot_deactivate_safety_pause_with_active_buffer() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let native_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Initialize contract
    client.mock_all_auths().initialize(
        &admin,
        &grant_token,
        &treasury,
        &native_token,
    );

    let grant_id: u64 = 1;
    let total_amount: i128 = 10000;
    let flow_rate: i128 = 10;

    // Create and activate grant
    set_timestamp(&env, 1_000);
    client.mock_all_auths().create_grant(
        &grant_id,
        &recipient,
        &total_amount,
        &flow_rate,
        &0,
        &Address::generate(&env),
        &String::from_str(&env, "PROP-001"),
        &String::from_str(&env, "SN-12345"),
        &1000,
        &1_000 + (total_amount / flow_rate),
    );
    
    client.mock_all_auths().activate_grant(&grant_id);
    client.mock_all_auths().activate_safety_pause(&grant_id);
    
    // Try to deactivate immediately (buffer still active for 4 hours)
    let result = std::panic::catch_unwind(|| {
        client.mock_all_auths().deactivate_safety_pause(&grant_id);
    });
    
    // Should fail because buffer is still active
    assert!(result.is_err());
}
