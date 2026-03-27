#[cfg(test)]
mod test_reputation_scoring {
    use super::*;
    use soroban_sdk::{testutils::Address as _, vec, Address, Env, Symbol};

    #[test]
    fn test_register_external_contract() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_address = Address::generate(&env);

        // Initialize contract
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let oracle = Address::generate(&env);
        let native_token = Address::generate(&env);

        GrantContract::initialize(
            env.clone(),
            admin.clone(),
            grant_token,
            treasury,
            oracle,
            native_token,
        )
        .unwrap();

        // Register external contract
        let contract_config = ExternalContractQuery {
            contract_address: contract_address.clone(),
            query_function: Symbol::new(&env, "get_completion_status"),
            project_name: "Stream-Scholar".to_string(),
            weight: 50,
        };

        GrantContract::register_external_contract(
            env.clone(),
            admin.clone(),
            contract_config,
        )
        .unwrap();

        // Verify contract was registered
        let contracts = env.storage().instance().get(&DataKey::ExternalContracts)
            .unwrap_or(vec![&env]);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts.get(0).unwrap().contract_address, contract_address);
    }

    #[test]
    fn test_reputation_score_calculation() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let contract_address = Address::generate(&env);

        // Initialize contract
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let oracle = Address::generate(&env);
        let native_token = Address::generate(&env);

        GrantContract::initialize(
            env.clone(),
            admin.clone(),
            grant_token,
            treasury,
            oracle,
            native_token,
        )
        .unwrap();

        // Register external contract
        let contract_config = ExternalContractQuery {
            contract_address: contract_address.clone(),
            query_function: Symbol::new(&env, "get_completion_status"),
            project_name: "Stream-Scholar".to_string(),
            weight: 100,
        };

        GrantContract::register_external_contract(
            env.clone(),
            admin.clone(),
            contract_config,
        )
        .unwrap();

        // Mock the external contract call (in real scenario, this would be a cross-contract call)
        // For testing, we'll simulate by directly setting cache
        write_reputation_cache(&env, &user, &contract_address, true);

        // Calculate reputation score
        let score = GrantContract::get_reputation_score(env.clone(), user.clone()).unwrap();

        assert_eq!(score.user, user);
        assert_eq!(score.total_completions, 1);
        assert_eq!(score.average_score, 100);
        assert_eq!(score.projects_completed.len(), 1);
    }

    #[test]
    fn test_stake_discount_calculation() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let contract_address = Address::generate(&env);

        // Initialize contract
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let oracle = Address::generate(&env);
        let native_token = Address::generate(&env);

        GrantContract::initialize(
            env.clone(),
            admin.clone(),
            grant_token,
            treasury,
            oracle,
            native_token,
        )
        .unwrap();

        // Register external contract
        let contract_config = ExternalContractQuery {
            contract_address: contract_address.clone(),
            query_function: Symbol::new(&env, "get_completion_status"),
            project_name: "Stream-Scholar".to_string(),
            weight: 100,
        };

        GrantContract::register_external_contract(
            env.clone(),
            admin.clone(),
            contract_config,
        )
        .unwrap();

        // Mock completion
        write_reputation_cache(&env, &user, &contract_address, true);

        // Calculate discount
        let discounted_amount = GrantContract::preview_stake_discount(env.clone(), user.clone()).unwrap();
        let base_amount = PROPOSAL_STAKE_AMOUNT;

        // Should have some discount (0.05 XLM for completion + 1 XLM for 100% score = 1.05 XLM discount)
        let expected_discount = 500_000 + 1_000_000; // 1.05 XLM in stroops
        assert_eq!(discounted_amount, base_amount - expected_discount);
    }

    #[test]
    fn test_stake_with_reputation_discount() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let staker = Address::generate(&env);
        let contract_address = Address::generate(&env);

        // Initialize contract
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let oracle = Address::generate(&env);
        let native_token = Address::generate(&env);

        GrantContract::initialize(
            env.clone(),
            admin.clone(),
            grant_token.clone(),
            treasury,
            oracle,
            native_token,
        )
        .unwrap();

        // Register external contract
        let contract_config = ExternalContractQuery {
            contract_address: contract_address.clone(),
            query_function: Symbol::new(&env, "get_completion_status"),
            project_name: "Stream-Scholar".to_string(),
            weight: 100,
        };

        GrantContract::register_external_contract(
            env.clone(),
            admin.clone(),
            contract_config,
        )
        .unwrap();

        // Mock completion for staker
        write_reputation_cache(&env, &staker, &contract_address, true);

        // Create a grant for staking
        let grant_id = 1;
        let recipient = Address::generate(&env);
        let total_amount = 100_000_000; // 10 XLM

        GrantContract::create_grant(
            env.clone(),
            admin.clone(),
            grant_id,
            recipient,
            total_amount,
            grant_token.clone(),
            0, // no cliff
            31536000, // 1 year duration
            0, // no acceleration
        )
        .unwrap();

        // Get discounted stake amount
        let discounted_amount = GrantContract::preview_stake_discount(env.clone(), staker.clone()).unwrap();

        // Mint tokens for staker
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&staker, &discounted_amount);

        // Deposit stake with discount
        GrantContract::deposit_proposal_stake(
            env.clone(),
            grant_id,
            staker.clone(),
            discounted_amount,
        )
        .unwrap();

        // Verify stake was recorded
        let stake = GrantContract::get_proposal_stake(env.clone(), grant_id).unwrap();
        assert_eq!(stake.staker, staker);
        assert_eq!(stake.amount, discounted_amount);
        assert_eq!(stake.status, StakeStatus::Deposited);
    }
}