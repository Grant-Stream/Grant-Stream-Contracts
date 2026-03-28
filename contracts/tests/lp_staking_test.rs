#[test]
fn test_lp_staking_as_collateral() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    // Setup test LP token (use mock or deploy SAC)
    let lp_token = ...; // Address of test LP token
    let grantee = Address::generate(&env);

    // Stake LP
    client.stake_lp_as_collateral(&grant_id, &lp_token, &1000u128, &None);

    // Complete milestones successfully
    // ...

    let bonus = client.claim_loyalty_bonus(&grant_id);
    assert!(bonus > 0);

    // Test slash scenario
}