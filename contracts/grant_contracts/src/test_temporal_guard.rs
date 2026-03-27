#![cfg(test)]

use soroban_sdk::{symbol_short, Address, Env, Vec};
use crate::temporal_guard::{
    TemporalGuardContract, TemporalGuardContractClient, InteractionType, 
    InteractionRecord, TemporalGuardError, TemporalGuardDataKey
};

#[test]
fn test_temporal_guard_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    // Test successful initialization
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(2));
    
    // Verify admin is set
    let stored_admin = env.storage().instance().get(&TemporalGuardDataKey::Admin).unwrap();
    assert_eq!(stored_admin, admin);
    
    // Verify temporal separation is set
    let separation = env.storage().instance().get(&TemporalGuardDataKey::TemporalSeparationRequired).unwrap();
    assert_eq!(separation, 2);
}

#[test]
fn test_temporal_guard_double_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    // First initialization should succeed
    client.initialize(&admin, Some(1));
    
    // Second initialization should fail
    let result = client.try_initialize(&admin, Some(1));
    assert_eq!(result, Err(TemporalGuardError::NotInitialized));
}

#[test]
fn test_withdrawal_allowed_first_time() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(1));
    
    // First withdrawal should be allowed
    let result = client.try_check_withdraw_allowed(&user, &123);
    assert_eq!(result, Ok(()));
}

#[test]
fn test_vote_allowed_first_time() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(1));
    
    // First vote should be allowed
    let result = client.try_check_vote_allowed(&user, &456);
    assert_eq!(result, Ok(()));
}

#[test]
fn test_same_ledger_cross_type_interaction_blocked() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(1));
    
    // Simulate same ledger by setting ledger sequence
    env.ledger().set_sequence(100);
    
    // First, record a withdrawal
    client.record_withdrawal(&user, &123);
    
    // Then try to vote in the same ledger - should be blocked
    let result = client.try_check_vote_allowed(&user, &456);
    assert_eq!(result, Err(TemporalGuardError::SameLedgerInteraction));
}

#[test]
fn test_same_ledger_same_type_interaction_allowed() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(1));
    
    // Simulate same ledger by setting ledger sequence
    env.ledger().set_sequence(100);
    
    // First, record a withdrawal
    client.record_withdrawal(&user, &123);
    
    // Then try another withdrawal in the same ledger - should be allowed
    let result = client.try_check_withdraw_allowed(&user, &124);
    assert_eq!(result, Ok(()));
}

#[test]
fn test_temporal_separation_enforcement() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(2)); // Require 2 ledger separation
    
    // First ledger
    env.ledger().set_sequence(100);
    client.record_withdrawal(&user, &123);
    
    // Next ledger (sequence 101) - should still be blocked due to 2-ledger requirement
    env.ledger().set_sequence(101);
    let result = client.try_check_vote_allowed(&user, &456);
    assert_eq!(result, Err(TemporalGuardError::SameLedgerInteraction));
    
    // Two ledgers later (sequence 102) - should be allowed
    env.ledger().set_sequence(102);
    let result = client.try_check_vote_allowed(&user, &456);
    assert_eq!(result, Ok(()));
}

#[test]
fn test_flash_loan_detection() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(1));
    
    // Simulate rapid interactions in different ledgers but close in time
    env.ledger().set_sequence(100);
    env.ledger().set_timestamp(1000);
    client.record_withdrawal(&user, &123);
    
    // Next ledger but only 2 seconds later (within 5-second flash loan window)
    env.ledger().set_sequence(101);
    env.ledger().set_timestamp(1002);
    let result = client.try_check_vote_allowed(&user, &456);
    assert_eq!(result, Err(TemporalGuardError::FlashLoanDetected));
}

#[test]
fn test_interaction_recording() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(1));
    
    env.ledger().set_sequence(100);
    env.ledger().set_timestamp(1000);
    
    // Record a withdrawal
    client.record_withdrawal(&user, &123);
    
    // Check that the interaction was recorded
    let last_interaction = client.get_last_interaction(&user).unwrap().unwrap();
    assert_eq!(last_interaction.interaction_type, InteractionType::Withdraw);
    assert_eq!(last_interaction.ledger, 100);
    assert_eq!(last_interaction.timestamp, 1000);
    assert_eq!(last_interaction.grant_id, Some(123));
    assert_eq!(last_interaction.proposal_id, None);
    
    // Record a vote
    client.record_vote(&user, &456);
    
    // Check that the interaction was updated
    let last_interaction = client.get_last_interaction(&user).unwrap().unwrap();
    assert_eq!(last_interaction.interaction_type, InteractionType::Vote);
    assert_eq!(last_interaction.ledger, 100);
    assert_eq!(last_interaction.timestamp, 1000);
    assert_eq!(last_interaction.grant_id, None);
    assert_eq!(last_interaction.proposal_id, Some(456));
}

#[test]
fn test_flash_loan_suspect_flag() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(1));
    
    // Initially should not be a suspect
    assert_eq!(client.is_flash_loan_suspect(&user), Ok(false));
    
    // Trigger flash loan detection
    env.ledger().set_sequence(100);
    env.ledger().set_timestamp(1000);
    client.record_withdrawal(&user, &123);
    
    env.ledger().set_sequence(101);
    env.ledger().set_timestamp(1002);
    let _ = client.try_check_vote_allowed(&user, &456);
    
    // Should now be flagged as suspect
    assert_eq!(client.is_flash_loan_suspect(&user), Ok(true));
}

#[test]
fn test_temporal_separation_update() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(1));
    
    // Update temporal separation
    client.update_temporal_separation(&admin, &5);
    
    // Verify the update
    let separation = env.storage().instance().get(&TemporalGuardDataKey::TemporalSeparationRequired).unwrap();
    assert_eq!(separation, 5);
}

#[test]
fn test_temporal_separation_update_unauthorized() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(1));
    
    // Unauthorized update should fail
    let result = client.try_update_temporal_separation(&unauthorized, &5);
    assert_eq!(result, Err(TemporalGuardError::Unauthorized));
}

#[test]
fn test_complex_interaction_sequence() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(1));
    
    // Ledger 100: Withdraw from grant 123
    env.ledger().set_sequence(100);
    env.ledger().set_timestamp(1000);
    client.record_withdrawal(&user, &123);
    
    // Ledger 101: Try to vote on proposal 456 - should be blocked (same ledger cross-type)
    env.ledger().set_sequence(101);
    let result = client.try_check_vote_allowed(&user, &456);
    assert_eq!(result, Err(TemporalGuardError::SameLedgerInteraction));
    
    // Ledger 102: Try to withdraw from grant 124 - should be allowed (same type)
    let result = client.try_check_withdraw_allowed(&user, &124);
    assert_eq!(result, Ok(()));
    client.record_withdrawal(&user, &124);
    
    // Ledger 103: Try to vote on proposal 456 - should be allowed (different ledger, sufficient separation)
    env.ledger().set_sequence(103);
    let result = client.try_check_vote_allowed(&user, &456);
    assert_eq!(result, Ok(()));
    client.record_vote(&user, &456);
    
    // Ledger 104: Try to withdraw from grant 125 - should be blocked (same ledger cross-type)
    env.ledger().set_sequence(104);
    let result = client.try_check_withdraw_allowed(&user, &125);
    assert_eq!(result, Err(TemporalGuardError::SameLedgerInteraction));
}

#[test]
fn test_multiple_users_independent() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(1));
    
    env.ledger().set_sequence(100);
    
    // User1 withdraws
    client.record_withdrawal(&user1, &123);
    
    // User2 should still be able to vote (different user)
    let result = client.try_check_vote_allowed(&user2, &456);
    assert_eq!(result, Ok(()));
    
    // User1 should be blocked from voting in same ledger
    let result = client.try_check_vote_allowed(&user1, &456);
    assert_eq!(result, Err(TemporalGuardError::SameLedgerInteraction));
}

#[test]
fn test_edge_case_zero_temporal_separation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    
    let contract_id = env.register_contract(None, TemporalGuardContract);
    let client = TemporalGuardContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(0)); // Zero separation should fail
    
    // Should reject zero separation
    let result = client.try_update_temporal_separation(&admin, &0);
    assert_eq!(result, Err(TemporalGuardError::InvalidAddress)); // Reusing error for invalid parameter
}
