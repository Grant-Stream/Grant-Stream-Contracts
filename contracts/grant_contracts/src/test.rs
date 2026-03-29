#![cfg(test)]
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

#[test]
fn test_grant_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token = Address::generate(&env);
    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.initialize(&admin, &token, &admin, &admin);
    client.create_grant(&1, &recipient, &1000, &SCALING_FACTOR, &0, &None, &false);

    env.ledger().with_mut(|li| li.timestamp = 10);
    assert_eq!(client.claimable(&1), 10);
    
    client.withdraw(&1, &5);
    assert_eq!(client.get_grant(&1).withdrawn, 5);
}