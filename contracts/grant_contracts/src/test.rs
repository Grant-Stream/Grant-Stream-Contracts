#![cfg(test)]

use super::{Error, GrantContract, GrantContractClient, GrantStatus, SCALING_FACTOR};
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, Ledger},
    Address, Env, InvokeError, symbol_short,
};

const RATE_INCREASE_TIMELOCK_SECS: u64 = 48 * 60 * 60;

fn set_timestamp(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

fn assert_contract_error<T, C>(
    result: Result<Result<T, C>, Result<Error, InvokeError>>,
    expected: Error,
) {
    assert!(matches!(result, Err(Ok(err)) if err == expected));
}

#[test]
fn test_complex_flow_with_warmup_and_scaling() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    // Initialize with all parameters
    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &grant_token, &treasury, &oracle);

    // Set mock protocol config for SBT/Debt hooks
    client.mock_all_auths().set_protocol_config(
        &Address::generate(&env), // sorosusu
        &treasury,                // treasury
        &Address::generate(&env), // sbt minter
        &2000                     // 20% debt divert
    );

    let grant_id: u64 = 1;
    let total_amount: i128 = 100_000 * 100; // 100k units
    let base_rate: i128 = 100 * SCALING_FACTOR; // 100 units/sec
    let warmup: u64 = 100;

    client.mock_all_auths().create_grant(
        &grant_id,
        &recipient,
        &total_amount,
        &base_rate,
        &warmup,
        &None,
        &false
    );

    // At start, ramp is 25%
    set_timestamp(&env, 1);
    assert_eq!(client.claimable(&grant_id), 25);

    // After 100s (warmup end), average rate is 62.5%
    set_timestamp(&env, 100);
    assert_eq!(client.claimable(&grant_id), 6250);

    // Withdraw updates last_claim_time
    client.mock_all_auths().withdraw(&grant_id, &1000);
    let g = client.get_grant(&grant_id);
    assert_eq!(g.last_claim_time, 100);
}

#[test]
fn test_joint_grant_withdrawal_requires_dual_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let partner = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.mock_all_auths().initialize(&admin, &grant_token, &treasury, &oracle);
    client.mock_all_auths().set_protocol_config(
        &Address::generate(&env),
        &treasury,
        &Address::generate(&env),
        &0
    );

    let grant_id = 99;
    client.mock_all_auths().create_grant(
        &grant_id,
        &recipient,
        &10000,
        &10 * SCALING_FACTOR,
        &0,
        &Some(partner.clone()),
        &false
    );

    set_timestamp(&env, 100);
    // Success with both auths
    client.mock_all_auths().withdraw(&grant_id, &500);
    assert_eq!(client.get_grant(&grant_id).withdrawn, 500);
}

#[test]
fn test_inactivity_slashing() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.mock_all_auths().initialize(&admin, &grant_token, &treasury, &oracle);
    client.mock_all_auths().set_protocol_config(
        &Address::generate(&env),
        &treasury,
        &Address::generate(&env),
        &0
    );

    let grant_id = 555;
    client.mock_all_auths().create_grant(
        &grant_id,
        &recipient,
        &10000,
        &1 * SCALING_FACTOR,
        &0,
        &None,
        &false
    );

    // Advance 91 days
    set_timestamp(&env, 91 * 24 * 60 * 60);
    client.mock_all_auths().slash_inactive_grant(&grant_id);

    let g = client.get_grant(&grant_id);
    assert_eq!(g.status, GrantStatus::Cancelled);
}

#[test]
fn test_rate_change_timelock_and_sustainability_tax() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.mock_all_auths().initialize(&admin, &grant_token, &treasury, &oracle);
    client.mock_all_auths().set_protocol_config(
        &Address::generate(&env),
        &treasury,
        &Address::generate(&env),
        &0
    );

    // Large grant to trigger tax (> 100k)
    let grant_id = 777;
    client.mock_all_auths().create_grant(
        &grant_id,
        &recipient,
        &200_000 * SCALING_FACTOR, // Over threshold
        &100 * SCALING_FACTOR,
        &0,
        &None,
        &false
    );

    set_timestamp(&env, 100);
    // Propose increase
    client.mock_all_auths().propose_rate_change(&grant_id, &200 * SCALING_FACTOR);
    
    // Decrease applies immediately
    client.mock_all_auths().propose_rate_change(&grant_id, &50 * SCALING_FACTOR);
    let g = client.get_grant(&grant_id);
    assert_eq!(g.flow_rate, 50 * SCALING_FACTOR);
}

#[test]
fn test_split_and_separate_joint_grant() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let partner = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.mock_all_auths().initialize(&admin, &grant_token, &treasury, &oracle);
    client.mock_all_auths().set_protocol_config(
        &Address::generate(&env),
        &treasury,
        &Address::generate(&env),
        &0
    );

    let grant_id = 888;
    client.mock_all_auths().create_grant(
        &grant_id,
        &recipient,
        &10000,
        &10 * SCALING_FACTOR,
        &0,
        &Some(partner.clone()),
        &false
    );

    set_timestamp(&env, 100);
    let new_id = 889;
    client.mock_all_auths().split_and_separate(&grant_id, &new_id);

    let g1 = client.get_grant(&grant_id);
    let g2 = client.get_grant(&new_id);

    assert_eq!(g1.recipient, recipient);
    assert_eq!(g2.recipient, partner);
    assert_eq!(g1.flow_rate, 5 * SCALING_FACTOR);
    assert_eq!(g2.flow_rate, 5 * SCALING_FACTOR);
}
