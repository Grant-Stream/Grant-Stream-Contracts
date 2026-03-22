#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token, Address, Env, Symbol,
    };

    fn set_timestamp(env: &Env, timestamp: u64) {
        env.ledger().with_mut(|li| {
            li.timestamp = timestamp;
        });
    }

    #[test]
    fn test_completion_certificate_nft_minting() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
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
            &oracle,
            &native_token,
        );

        let grant_id: u64 = 1;
        let total_amount: i128 = 1000;
        let flow_rate: i128 = 10;

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &0);

        // Verify no NFT exists initially
        assert!(client.try_completion_nft_token_id(&grant_id).is_err());

        // Fast forward to complete the grant
        set_timestamp(&env, 1_000 + (total_amount / flow_rate) + 10);

        // Withdraw to trigger completion and NFT minting
        client.mock_all_auths().withdraw(&grant_id, &total_amount);

        // Verify NFT was minted
        let nft_token_id = client.completion_nft_token_id(&grant_id);
        assert!(nft_token_id.is_ok());

        // Verify NFT ownership
        let token_id = nft_token_id.unwrap();
        let nft_owner = client.nft_owner_of(&token_id);
        assert!(nft_owner.is_ok());
        assert_eq!(nft_owner.unwrap(), recipient);

        // Verify token count increased
        let token_count = client.nft_token_count();
        assert_eq!(token_count, 1);

        // Verify grant is completed
        let grant = client.get_grant(&grant_id);
        assert!(grant.is_ok());
        assert_eq!(grant.unwrap().status, GrantStatus::Completed);
    }

    #[test]
    fn test_nft_metadata_generation() {
        let env = Env::default();
        let recipient = Address::generate(&env);
        
        let grant_id: u64 = 42;
        let total_amount: i128 = 5000;
        let token_symbol = "USDC";
        let dao_name = "Stellar DAO";
        let repo_url = "https://github.com/example/project";

        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);

        let metadata = client.nft_metadata(
            &grant_id,
            &recipient,
            &total_amount,
            token_symbol,
            dao_name,
            repo_url,
        );

        // Verify metadata contains expected fields
        let metadata_str = metadata.to_string();
        assert!(metadata_str.contains("Stellar Grant Completion Certificate"));
        assert!(metadata_str.contains(&grant_id.to_string()));
        assert!(metadata_str.contains(&total_amount.to_string()));
        assert!(metadata_str.contains("USDC"));
        assert!(metadata_str.contains("Stellar DAO"));
        assert!(metadata_str.contains("certificate_type"));
        assert!(metadata_str.contains("grant_completion"));
    }

    #[test]
    fn test_no_duplicate_nft_minting() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
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
            &oracle,
            &native_token,
        );

        let grant_id: u64 = 1;
        let total_amount: i128 = 1000;
        let flow_rate: i128 = 10;

        // Create and complete grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &0);

        set_timestamp(&env, 1_000 + (total_amount / flow_rate) + 10);
        client.mock_all_auths().withdraw(&grant_id, &total_amount);

        // Verify NFT was minted once
        let first_token_id = client.completion_nft_token_id(&grant_id).unwrap();
        let token_count = client.nft_token_count();
        assert_eq!(token_count, 1);

        // Try to withdraw again (should not mint another NFT)
        set_timestamp(&env, 1_000 + (total_amount / flow_rate) + 20);
        
        // This should fail because grant is already completed and no more funds are claimable
        assert!(client.mock_all_auths().try_withdraw(&grant_id, &1).is_err());

        // Verify still only one NFT exists
        let token_count = client.nft_token_count();
        assert_eq!(token_count, 1);
        
        let second_token_id = client.completion_nft_token_id(&grant_id).unwrap();
        assert_eq!(first_token_id, second_token_id);
    }
}
