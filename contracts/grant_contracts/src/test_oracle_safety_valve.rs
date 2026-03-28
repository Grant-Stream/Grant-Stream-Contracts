#![cfg(test)]
extern crate std;

use crate::{Error, GrantContract, GrantContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup(env: &Env) -> (GrantContractClient<'_>, Address, Address) {
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    let treasury = Address::generate(env);
    let grant_token = Address::generate(env);
    let native_token = Address::generate(env);

    client.initialize(&admin, &grant_token, &treasury, &oracle, &native_token);
    (client, admin, oracle)
}

/// Oracle heartbeat records a timestamp; a fresh heartbeat blocks proposals.
#[test]
fn test_heartbeat_blocks_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _oracle) = setup(&env);

    // Set total voting power so proposals can be created
    // (reuse set_total_voting_power via admin path — here we just test the guard)
    client.oracle_heartbeat();

    let proposer = Address::generate(&env);
    let result = client.try_propose_safety_rate(&proposer, &105, &100);
    assert_eq!(result, Err(Ok(Error::OracleStillActive)));
}

/// After 48 h of silence a proposal can be created and executed with 90% votes.
#[test]
fn test_full_safety_valve_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _oracle) = setup(&env);

    // Advance time past the 48-hour window without a heartbeat
    env.ledger().with_mut(|l| l.timestamp = 48 * 60 * 60 + 1);

    // Seed total voting power (100 units)
    // The contract reads TotalVotingPower; set it directly via the existing
    // set_voting_power path used in slashing tests.
    let voter_a = Address::generate(&env);
    let voter_b = Address::generate(&env);
    client.set_voting_power(&voter_a, &90);
    client.set_voting_power(&voter_b, &10);
    client.set_total_voting_power(&100);

    // Propose a manual rate of 1.05 (105/100)
    let proposal_id = client.propose_safety_rate(&voter_a, &105, &100);
    assert_eq!(proposal_id, 0);

    // Advance past the voting deadline
    env.ledger().with_mut(|l| l.timestamp += 7 * 24 * 60 * 60 + 1);

    // voter_a (90 power) votes yes — that's 90% of 100 total
    client.vote_on_safety_rate(&voter_a, &proposal_id, &true);

    // Execute: 90/100 = 90% ≥ threshold
    client.execute_safety_rate(&proposal_id);

    let (num, den) = client.get_exchange_rate();
    assert_eq!(num, 105);
    assert_eq!(den, 100);
}

/// Execution fails when approval is below 90%.
#[test]
fn test_safety_valve_rejected_below_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _oracle) = setup(&env);

    env.ledger().with_mut(|l| l.timestamp = 48 * 60 * 60 + 1);

    let voter_a = Address::generate(&env);
    let voter_b = Address::generate(&env);
    client.set_voting_power(&voter_a, &89);
    client.set_voting_power(&voter_b, &11);
    client.set_total_voting_power(&100);

    let proposal_id = client.propose_safety_rate(&voter_a, &110, &100);

    env.ledger().with_mut(|l| l.timestamp += 7 * 24 * 60 * 60 + 1);

    // Only 89% approval — below the 90% threshold
    client.vote_on_safety_rate(&voter_a, &proposal_id, &true);
    client.vote_on_safety_rate(&voter_b, &proposal_id, &false);

    let result = client.try_execute_safety_rate(&proposal_id);
    assert_eq!(result, Err(Ok(Error::SafetyApprovalThresholdNotMet)));
}

/// If the oracle sends a heartbeat after a proposal is created, execution is blocked.
#[test]
fn test_oracle_recovery_blocks_execution() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _oracle) = setup(&env);

    env.ledger().with_mut(|l| l.timestamp = 48 * 60 * 60 + 1);

    let voter = Address::generate(&env);
    client.set_voting_power(&voter, &100);
    client.set_total_voting_power(&100);

    let proposal_id = client.propose_safety_rate(&voter, &105, &100);

    // Oracle comes back online
    env.ledger().with_mut(|l| l.timestamp += 1);
    client.oracle_heartbeat();

    // Advance past voting deadline
    env.ledger().with_mut(|l| l.timestamp += 7 * 24 * 60 * 60 + 1);
    client.vote_on_safety_rate(&voter, &proposal_id, &true);

    // Execution should fail because oracle is now active
    let result = client.try_execute_safety_rate(&proposal_id);
    assert_eq!(result, Err(Ok(Error::OracleStillActive)));
}
