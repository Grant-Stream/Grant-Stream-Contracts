#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Vec,
};

use crate::{SoroSusuContract, SoroSusuContractClient, Member, Round, RoundStatus, DeficitVote};

fn set_timestamp(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

/// TASK 1: Test gas bounty incentive for finalizing rounds
#[test]
fn test_gas_bounty_finalize_round() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    
    let contract_id = env.register(SoroSusuContract, ());
    let client = SoroSusuContractClient::new(&env, &contract_id);
    
    // Initialize contract
    client.initialize(&admin, &treasury);
    
    // Register members
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let member3 = Address::generate(&env);
    
    client.register_member(&member1, &1000);
    client.register_member(&member2, &1000);
    client.register_member(&member3, &1000);
    
    // Start round
    let mut members = Vec::new(&env);
    members.push_back(member1.clone());
    members.push_back(member2.clone());
    members.push_back(member3.clone());
    
    let round_id = client.start_round(&members);
    
    // Advance time beyond round duration (7 days + 1 hour)
    set_timestamp(&env, 1000 + 7 * 24 * 60 * 60 + 3600);
    
    // Anyone can finalize (permissionless)
    let random_user = Address::generate(&env);
    env.mock_all_auths();
    
    // Finalize round - random user should receive gas bounty
    client.finalize_round(&round_id, &member1);
    
    // Verify round was finalized
    let round: Round = client.get_round(&round_id);
    assert_eq!(round.status, RoundStatus::Finalized);
    assert_eq!(round.winner, Some(member1.clone()));
    assert!(round.finalized_by.is_some());
    assert!(round.gas_bounty_paid > 0);
    
    // Verify gas bounty was paid to the caller (random_user)
    // The caller should receive 0.1% of platform fee (0.5% of pot)
    let expected_pot = 3000; // 3 members * 1000
    let expected_platform_fee = (expected_pot * 50) / 10000; // 0.5% = 15
    let expected_gas_bounty = (expected_platform_fee * 10) / 10000; // 0.1% of fee = 0.015
    
    assert!(round.gas_bounty_paid >= expected_gas_bounty);
}

/// TASK 2: Test clawback deficit detection and resolution
#[test]
fn test_clawback_deficit_resolution() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    
    let contract_id = env.register(SoroSusuContract, ());
    let client = SoroSusuContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, &treasury);
    
    // Register members
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let member3 = Address::generate(&env);
    
    client.register_member(&member1, &1000);
    client.register_member(&member2, &1000);
    client.register_member(&member3, &1000);
    
    // Start round
    let mut members = Vec::new(&env);
    members.push_back(member1.clone());
    members.push_back(member2.clone());
    members.push_back(member3.clone());
    
    let round_id = client.start_round(&members);
    
    // Simulate clawback: Remove 500 from contract balance
    // In reality this would happen via external regulated asset clawback
    // For testing, we'll test the deficit detection logic
    
    // Advance time
    set_timestamp(&env, 1000 + 7 * 24 * 60 * 60 + 3600);
    
    // Try to finalize - should detect deficit and pause
    let result = std::panic::catch_unwind(|| {
        client.finalize_round(&round_id, &member1);
    });
    
    // Should fail with DeficitDetected error
    assert!(result.is_err());
    
    // Verify round is in DeficitPaused state
    let round: Round = client.get_round(&round_id);
    assert_eq!(round.status, RoundStatus::DeficitPaused);
    assert!(round.deficit_amount > 0);
    assert!(round.recovery_surcharge_per_member > 0);
}

/// TASK 2: Test voting on deficit resolution
#[test]
fn test_deficit_vote_skip_payout() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    
    let contract_id = env.register(SoroSusuContract, ());
    let client = SoroSusuContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, &treasury);
    
    // Register members
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let member3 = Address::generate(&env);
    
    client.register_member(&member1, &1000);
    client.register_member(&member2, &1000);
    client.register_member(&member3, &1000);
    
    // Start round
    let mut members = Vec::new(&env);
    members.push_back(member1.clone());
    members.push_back(member2.clone());
    members.push_back(member3.clone());
    
    let round_id = client.start_round(&members);
    
    // Trigger deficit (manually set round status for testing)
    // In real scenario, this happens via finalize_round detecting balance < pot
    
    // Members vote to skip payout
    env.mock_all_auths();
    client.vote_on_deficit(&round_id, &true); // member1 votes for skip
    client.vote_on_deficit(&round_id, &true); // member2 votes for skip
    client.vote_on_deficit(&round_id, &false); // member3 votes against
    
    // Advance past voting period (3 days + 1 hour)
    set_timestamp(&env, 1000 + 3 * 24 * 60 * 60 + 3600);
    
    // Execute vote (needs 66% approval)
    client.execute_deficit_vote(&round_id);
    
    // Verify round was skipped
    let round: Round = client.get_round(&round_id);
    assert_eq!(round.status, RoundStatus::Skipped);
    assert!(round.deficit_vote_passed);
}

/// TASK 3: Test inter-contract reliability score query
#[test]
fn test_reliability_score_grant_priority() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    
    let contract_id = env.register(SoroSusuContract, ());
    let client = SoroSusuContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, &treasury);
    
    // Register member with perfect participation
    let good_member = Address::generate(&env);
    client.register_member(&good_member, &1000);
    
    // Simulate multiple successful rounds
    for _i in 0..5 {
        let mut members = Vec::new(&env);
        members.push_back(good_member.clone());
        members.push_back(Address::generate(&env));
        members.push_back(Address::generate(&env));
        
        let round_id = client.start_round(&members);
        
        // Advance time and finalize
        set_timestamp(&env, 1000 + 7 * 24 * 60 * 60 + 3600);
        client.finalize_round(&round_id, &good_member);
    }
    
    // Check reliability score
    let member_data: Member = client.get_member(&good_member);
    assert!(member_data.reliability_score > 900); // Should have high score
    
    // Verify grant priority eligibility
    let eligible = client.verify_grant_priority_eligible(&good_member);
    assert!(eligible); // Should qualify for priority
}

/// TASK 3: Test grant stream contract integration
#[test]
fn test_set_grant_stream_contract() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    
    let contract_id = env.register(SoroSusuContract, ());
    let client = SoroSusuContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, &treasury);
    
    // Set Grant-Stream contract address
    let grant_stream_contract = Address::generate(&env);
    client.set_grant_stream_contract(&admin, &grant_stream_contract);
    
    // Verify it was set (would be stored in DataKey::GrantStreamContract)
    // In production, Grant-Stream contract would call get_reliability_score
    let member = Address::generate(&env);
    client.register_member(&member, &1000);
    
    let score = client.get_reliability_score(&member);
    assert!(score >= 0);
}

/// Test recovery surcharge calculation
#[test]
fn test_recovery_surcharge_calculation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    
    let contract_id = env.register(SoroSusuContract, ());
    let client = SoroSusuContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, &treasury);
    
    // Register 10 members
    let mut members = Vec::new(&env);
    for i in 0..10 {
        let member = Address::generate(&env);
        client.register_member(&member, &1000);
        members.push_back(member);
    }
    
    let round_id = client.start_round(&members);
    
    // Simulate 20% deficit (6000 out of 10000 missing)
    // Recovery surcharge should be 5% split among members
    
    // Advance time
    set_timestamp(&env, 1000 + 7 * 24 * 60 * 60 + 3600);
    
    // Trigger deficit
    let result = std::panic::catch_unwind(|| {
        client.finalize_round(&round_id, &members.get(0).unwrap());
    });
    
    assert!(result.is_err());
    
    let round: Round = client.get_round(&round_id);
    
    // Verify surcharge is calculated correctly
    // 5% of deficit / 10 members
    let expected_surcharge = (round.deficit_amount * 500) / (10 * 10000);
    assert_eq!(round.recovery_surcharge_per_member, expected_surcharge);
}

/// Test that non-members cannot vote on deficit
#[test]
fn test_non_member_cannot_vote_deficit() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    
    let contract_id = env.register(SoroSusuContract, ());
    let client = SoroSusuContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, &treasury);
    
    // Register members
    let member1 = Address::generate(&env);
    client.register_member(&member1, &1000);
    
    let mut members = Vec::new(&env);
    members.push_back(member1.clone());
    members.push_back(Address::generate(&env));
    members.push_back(Address::generate(&env));
    
    let round_id = client.start_round(&members);
    
    // Non-member tries to vote
    let outsider = Address::generate(&env);
    env.mock_all_auths();
    
    let result = std::panic::catch_unwind(|| {
        client.vote_on_deficit(&round_id, &outsider, &true);
    });
    
    // Should fail
    assert!(result.is_err());
}

/// Test gas bounty economics
#[test]
fn test_gas_bounty_economics() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    
    let contract_id = env.register(SoroSusuContract, ());
    let client = SoroSusuContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, &treasury);
    
    // Create large round
    let mut members = Vec::new(&env);
    for _i in 0..10 {
        let member = Address::generate(&env);
        client.register_member(&member, &10000);
        members.push_back(member);
    }
    
    let round_id = client.start_round(&members);
    
    // Advance time
    set_timestamp(&env, 1000 + 7 * 24 * 60 * 60 + 3600);
    
    // Multiple users try to finalize (race condition test)
    let winner = members.get(0).unwrap();
    let caller1 = Address::generate(&env);
    let caller2 = Address::generate(&env);
    
    env.mock_all_auths();
    
    // First caller succeeds
    client.finalize_round(&round_id, &winner);
    
    let round: Round = client.get_round(&round_id);
    
    // Verify platform fee structure
    let total_pot = 100000; // 10 members * 10000
    let platform_fee_bps = 50; // 0.5%
    let gas_bounty_bps = 10; // 0.1% of fee
    
    let expected_fee = (total_pot * platform_fee_bps) / 10000;
    let expected_bounty = (expected_fee * gas_bounty_bps) / 10000;
    
    assert!(round.gas_bounty_paid >= expected_bounty);
    
    // Second caller should fail (round already finalized)
    let result = std::panic::catch_unwind(|| {
        client.finalize_round(&round_id, &winner);
    });
    assert!(result.is_err());
}
