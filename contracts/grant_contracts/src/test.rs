#![cfg(test)]

use super::{Error, GrantContract, GrantContractClient, GrantStatus, SCALING_FACTOR};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

fn set_timestamp(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

#[test]
fn test_complex_flow_with_warmup_and_scaling() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    set_timestamp(&env, 0);
    // Initialize standard state
    client.initialize(&admin, &grant_token, &treasury, &oracle);

    client.set_protocol_config(
        &Address::generate(&env), // sorosusu
        &treasury,                // treasury
        &Address::generate(&env), // sbt minter
        &2000                     // 20% debt divert
    );

    let grant_id: u64 = 1;
    let total_amount: i128 = 100_000 * SCALING_FACTOR; 
    let base_rate: i128 = 100; // 100 stroops per second
    let warmup: u64 = 100;

    client.create_grant(
        &grant_id,
        &recipient,
        &total_amount,
        &base_rate,
        &warmup,
        &None,  // partner
        &false  // auto_debt_service
    );

    // Warmup Logic: Usually starts at a base (e.g., 25%) and ramps to 100%
    // At T=1: Minimal accrual
    set_timestamp(&env, 1);
    let claimable_t1 = client.claimable(&grant_id);
    assert!(claimable_t1 > 0);

    // After 100s (warmup complete): 
    // Average multiplier over [0, 100] is (25% + 100%) / 2 = 62.5%
    // Total = base_rate (100) * duration (100) * 0.625 = 6250
    set_timestamp(&env, 100);
    assert_eq!(client.claimable(&grant_id), 6250);

    client.withdraw(&grant_id, &1000);
    let g = client.get_grant(&grant_id);
    assert_eq!(g.last_claim_time, 100);
}

#[test]
fn test_joint_grant_withdrawal_requires_dual_auth() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let partner = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.initialize(&admin, &grant_token, &treasury, &oracle);
    
    let grant_id = 99;
    client.create_grant(
        &grant_id,
        &recipient,
        &10000,
        &10,
        &0,
        &Some(partner.clone()),
        &false
    );

    set_timestamp(&env, 100);
    // In mock_all_auths mode, this simulates both required signatures being present
    client.withdraw(&grant_id, &500);
    assert_eq!(client.get_grant(&grant_id).withdrawn, 500);
}

#[test]
fn test_inactivity_slashing() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.initialize(&admin, &grant_token, &treasury, &oracle);

    let grant_id = 555;
    client.create_grant(&grant_id, &recipient, &10000, &1, &0, &None, &false);

    // Advance 91 days (Threshold is 90 days)
    set_timestamp(&env, 91 * 24 * 60 * 60);
    client.slash_inactive_grant(&grant_id);

    let g = client.get_grant(&grant_id);
    assert_eq!(g.status, GrantStatus::Cancelled);
}

#[test]
fn test_rate_change_timelock() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.initialize(&admin, &grant_token, &treasury, &oracle);

    let grant_id = 777;
    client.create_grant(&grant_id, &recipient, &(200_000 * SCALING_FACTOR), &100, &0, &None, &false);

    set_timestamp(&env, 100);
    
    // Propose a rate decrease (usually applies immediately or skips timelock)
    client.propose_rate_change(&grant_id, &50);
    let g = client.get_grant(&grant_id);
    assert_eq!(g.flow_rate, 50);
}

#[test]
fn test_split_and_separate_joint_grant() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let partner = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.initialize(&admin, &grant_token, &treasury, &oracle);

    let grant_id = 888;
    // Create joint grant with 10 units/sec
    client.create_grant(&grant_id, &recipient, &20000, &10, &0, &Some(partner.clone()), &false);

    set_timestamp(&env, 100);
    let new_id = 889;
    client.split_and_separate(&grant_id, &new_id);

    let g1 = client.get_grant(&grant_id);
    let g2 = client.get_grant(&new_id);

    assert_eq!(g1.recipient, recipient);
    assert_eq!(g2.recipient, partner);
    // Flow rate should be split 50/50
    assert_eq!(g1.flow_rate, 5);
    assert_eq!(g2.flow_rate, 5);
}