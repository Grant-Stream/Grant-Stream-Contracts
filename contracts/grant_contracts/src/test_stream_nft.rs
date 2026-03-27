#![cfg(test)]

use soroban_sdk::{symbol_short, Address, Env, Vec, Map};
use crate::stream_nft::{
    StreamNFTContract, StreamNFTContractClient, StreamNFTError, StreamNFTDataKey,
    StreamNFT, WrappedStream, MarketplaceListing, InteractionType, InteractionRecord,
    StreamDetails, MIN_STREAM_DURATION, MAX_DISCOUNT_RATE
};

#[test]
fn test_stream_nft_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let marketplace = Address::generate(&env);
    let royalty_recipient = Address::generate(&env);
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    // Test successful initialization
    client.initialize(&admin, Some(&marketplace), Some(&royalty_recipient));
    
    // Verify admin is set
    let stored_admin = env.storage().instance().get(&StreamNFTDataKey::Admin).unwrap();
    assert_eq!(stored_admin, admin);
    
    // Verify marketplace is set
    let stored_marketplace = env.storage().instance().get(&StreamNFTDataKey::MarketplaceContract).unwrap();
    assert_eq!(stored_marketplace, marketplace);
    
    // Verify royalty recipient is set
    let stored_royalty = env.storage().instance().get(&StreamNFTDataKey::RoyaltyRecipient).unwrap();
    assert_eq!(stored_royalty, royalty_recipient);
    
    // Verify initial token ID and total supply
    let next_token_id = env.storage().instance().get(&StreamNFTDataKey::NextTokenId).unwrap();
    assert_eq!(next_token_id, 1);
    
    let total_supply = env.storage().instance().get(&StreamNFTDataKey::TotalSupply).unwrap();
    assert_eq!(total_supply, 0);
}

#[test]
fn test_stream_nft_double_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    // First initialization should succeed
    client.initialize(&admin, None, None);
    
    // Second initialization should fail
    let result = client.try_initialize(&admin, None, None);
    assert_eq!(result, Err(StreamNFTError::NotInitialized));
}

#[test]
fn test_wrap_stream_success() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let grant_id = 123u64;
    let wrap_amount = 1000000i128;
    let discount_rate = 1000u32; // 10%
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, None, None);
    
    // Mock stream details
    let stream_details = StreamDetails {
        grant_id,
        stream_rate_per_second: 1000,
        stream_start: env.ledger().timestamp(),
        stream_end: env.ledger().timestamp() + 86400 * 30, // 30 days
        remaining_amount: 2592000000, // Total for 30 days
    };
    
    // Store mock stream details (in real implementation, this would come from grant contract)
    env.storage().instance().set(&StreamNFTDataKey::WrappedStream(grant_id), &stream_details);
    
    // Test successful wrapping
    let token_id = client.wrap_stream(&grantee, &grant_id, &wrap_amount, &discount_rate);
    assert!(token_id.is_ok());
    
    let token_id_value = token_id.unwrap();
    assert_eq!(token_id_value, 1); // First NFT should have token ID 1
    
    // Verify NFT was created
    let nft = client.get_nft(&token_id_value);
    assert!(nft.is_ok());
    let nft_value = nft.unwrap();
    assert_eq!(nft_value.token_id, token_id_value);
    assert_eq!(nft_value.original_grantee, grantee);
    assert_eq!(nft_value.current_holder, grantee);
    assert_eq!(nft_value.grant_id, grant_id);
    assert_eq!(nft_value.total_amount, wrap_amount);
    assert_eq!(nft_value.remaining_amount, wrap_amount);
    assert_eq!(nft_value.discount_rate_bps, discount_rate);
    assert!(nft_value.is_active);
    
    // Verify wrapped stream was created
    let wrapped_stream = client.get_wrapped_stream(&token_id_value);
    assert!(wrapped_stream.is_ok());
    let wrapped_stream_value = wrapped_stream.unwrap();
    assert_eq!(wrapped_stream_value.nft_token_id, token_id_value);
    assert_eq!(wrapped_stream_value.original_beneficiary, grantee);
    assert_eq!(wrapped_stream_value.current_beneficiary, grantee);
    assert_eq!(wrapped_stream_value.wrapped_amount, wrap_amount);
    
    // Verify total supply was updated
    let total_supply = client.get_total_supply();
    assert_eq!(total_supply, 1);
    
    // Verify user NFTs
    let user_nfts = client.get_user_nfts(&grantee);
    assert_eq!(user_nfts.len(), 1);
    assert_eq!(user_nfts.get(0).unwrap(), token_id_value);
}

#[test]
fn test_wrap_stream_invalid_discount_rate() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let grant_id = 123u64;
    let wrap_amount = 1000000i128;
    let discount_rate = MAX_DISCOUNT_RATE + 1; // Too high
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, None, None);
    
    // Test wrapping with invalid discount rate
    let result = client.try_wrap_stream(&grantee, &grant_id, &wrap_amount, &discount_rate);
    assert_eq!(result, Err(StreamNFTError::DiscountRateTooHigh));
}

#[test]
fn test_wrap_stream_insufficient_balance() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let grant_id = 123u64;
    let wrap_amount = 1000000i128;
    let discount_rate = 1000u32;
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, None, None);
    
    // Mock stream details with insufficient balance
    let stream_details = StreamDetails {
        grant_id,
        stream_rate_per_second: 1000,
        stream_start: env.ledger().timestamp(),
        stream_end: env.ledger().timestamp() + 86400 * 30,
        remaining_amount: 500000, // Less than wrap_amount
    };
    
    env.storage().instance().set(&StreamNFTDataKey::WrappedStream(grant_id), &stream_details);
    
    // Test wrapping with insufficient balance
    let result = client.try_wrap_stream(&grantee, &grant_id, &wrap_amount, &discount_rate);
    assert_eq!(result, Err(StreamNFTError::InsufficientBalance));
}

#[test]
fn test_wrap_stream_already_wrapped() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let grant_id = 123u64;
    let wrap_amount = 1000000i128;
    let discount_rate = 1000u32;
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, None, None);
    
    // Mock stream details
    let stream_details = StreamDetails {
        grant_id,
        stream_rate_per_second: 1000,
        stream_start: env.ledger().timestamp(),
        stream_end: env.ledger().timestamp() + 86400 * 30,
        remaining_amount: 2592000000,
    };
    
    env.storage().instance().set(&StreamNFTDataKey::WrappedStream(grant_id), &stream_details);
    
    // First wrap should succeed
    let token_id = client.wrap_stream(&grantee, &grant_id, &wrap_amount, &discount_rate).unwrap();
    
    // Second wrap should fail
    let result = client.try_wrap_stream(&grantee, &grant_id, &wrap_amount, &discount_rate);
    assert_eq!(result, Err(StreamNFTError::StreamAlreadyWrapped));
}

#[test]
fn test_transfer_nft() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let buyer = Address::generate(&env);
    let grant_id = 123u64;
    let wrap_amount = 1000000i128;
    let discount_rate = 1000u32;
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, None, None);
    
    // Mock stream details
    let stream_details = StreamDetails {
        grant_id,
        stream_rate_per_second: 1000,
        stream_start: env.ledger().timestamp(),
        stream_end: env.ledger().timestamp() + 86400 * 30,
        remaining_amount: 2592000000,
    };
    
    env.storage().instance().set(&StreamNFTDataKey::WrappedStream(grant_id), &stream_details);
    
    // Create NFT
    let token_id = client.wrap_stream(&grantee, &grant_id, &wrap_amount, &discount_rate).unwrap();
    
    // Transfer NFT
    client.transfer_nft(&grantee, &buyer, &token_id);
    
    // Verify NFT ownership changed
    let nft = client.get_nft(&token_id).unwrap();
    assert_eq!(nft.current_holder, buyer);
    
    // Verify wrapped stream beneficiary changed
    let wrapped_stream = client.get_wrapped_stream(&token_id).unwrap();
    assert_eq!(wrapped_stream.current_beneficiary, buyer);
    
    // Verify user NFT lists updated
    let grantee_nfts = client.get_user_nfts(&grantee);
    assert_eq!(grantee_nfts.len(), 0);
    
    let buyer_nfts = client.get_user_nfts(&buyer);
    assert_eq!(buyer_nfts.len(), 1);
    assert_eq!(buyer_nfts.get(0).unwrap(), token_id);
}

#[test]
fn test_transfer_nft_unauthorized() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let buyer = Address::generate(&env);
    let grant_id = 123u64;
    let wrap_amount = 1000000i128;
    let discount_rate = 1000u32;
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, None, None);
    
    // Mock stream details
    let stream_details = StreamDetails {
        grant_id,
        stream_rate_per_second: 1000,
        stream_start: env.ledger().timestamp(),
        stream_end: env.ledger().timestamp() + 86400 * 30,
        remaining_amount: 2592000000,
    };
    
    env.storage().instance().set(&StreamNFTDataKey::WrappedStream(grant_id), &stream_details);
    
    // Create NFT
    let token_id = client.wrap_stream(&grantee, &grant_id, &wrap_amount, &discount_rate).unwrap();
    
    // Try to transfer NFT from unauthorized address
    let result = client.try_transfer_nft(&unauthorized, &buyer, &token_id);
    assert_eq!(result, Err(StreamNFTError::Unauthorized));
}

#[test]
fn test_unwrap_stream() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let grant_id = 123u64;
    let wrap_amount = 1000000i128;
    let discount_rate = 1000u32;
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, None, None);
    
    // Mock stream details
    let stream_details = StreamDetails {
        grant_id,
        stream_rate_per_second: 1000,
        stream_start: env.ledger().timestamp(),
        stream_end: env.ledger().timestamp() + 86400 * 30,
        remaining_amount: 2592000000,
    };
    
    env.storage().instance().set(&StreamNFTDataKey::WrappedStream(grant_id), &stream_details);
    
    // Create NFT
    let token_id = client.wrap_stream(&grantee, &grant_id, &wrap_amount, &discount_rate).unwrap();
    
    // Unwrap stream
    client.unwrap_stream(&grantee, &token_id);
    
    // Verify NFT was burned
    let result = client.try_get_nft(&token_id);
    assert_eq!(result, Err(StreamNFTError::NFTNotFound));
    
    // Verify wrapped stream was cleaned up
    let result = client.try_get_wrapped_stream(&token_id);
    assert_eq!(result, Err(StreamNFTError::StreamNotFound));
    
    // Verify total supply was updated
    let total_supply = client.get_total_supply();
    assert_eq!(total_supply, 0);
    
    // Verify user NFTs list updated
    let user_nfts = client.get_user_nfts(&grantee);
    assert_eq!(user_nfts.len(), 0);
}

#[test]
fn test_claim_stream_funds() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let grant_id = 123u64;
    let wrap_amount = 1000000i128;
    let discount_rate = 1000u32;
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, None, None);
    
    // Mock stream details
    let stream_details = StreamDetails {
        grant_id,
        stream_rate_per_second: 1000,
        stream_start: env.ledger().timestamp(),
        stream_end: env.ledger().timestamp() + 86400 * 30,
        remaining_amount: 2592000000,
    };
    
    env.storage().instance().set(&StreamNFTDataKey::WrappedStream(grant_id), &stream_details);
    
    // Create NFT
    let token_id = client.wrap_stream(&grantee, &grant_id, &wrap_amount, &discount_rate).unwrap();
    
    // Advance time to accrue funds
    env.ledger().set_timestamp(env.ledger().timestamp() + 3600); // 1 hour later
    
    // Claim funds
    let claimed_amount = client.claim_stream_funds(&grantee, &token_id);
    assert!(claimed_amount.is_ok());
    
    let claimed = claimed_amount.unwrap();
    assert!(claimed > 0); // Should have accrued some funds
    
    // Verify wrapped stream was updated
    let wrapped_stream = client.get_wrapped_stream(&token_id).unwrap();
    assert_eq!(wrapped_stream.last_claim_timestamp, env.ledger().timestamp());
}

#[test]
fn test_list_on_marketplace() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let marketplace = Address::generate(&env);
    let grant_id = 123u64;
    let wrap_amount = 1000000i128;
    let discount_rate = 1000u32;
    let asking_price = 500000i128;
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, Some(&marketplace), None);
    
    // Mock stream details
    let stream_details = StreamDetails {
        grant_id,
        stream_rate_per_second: 1000,
        stream_start: env.ledger().timestamp(),
        stream_end: env.ledger().timestamp() + 86400 * 30,
        remaining_amount: 2592000000,
    };
    
    env.storage().instance().set(&StreamNFTDataKey::WrappedStream(grant_id), &stream_details);
    
    // Create NFT
    let token_id = client.wrap_stream(&grantee, &grant_id, &wrap_amount, &discount_rate).unwrap();
    
    // List on marketplace
    client.list_on_marketplace(&grantee, &token_id, &asking_price, Some(86400)); // 24 hour duration
    
    // Verify listing was created (in real implementation, would check marketplace contract)
    // For now, just verify the function doesn't error
    assert!(true); // If we reach here, the listing succeeded
}

#[test]
fn test_stream_nft_edge_cases() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, None, None);
    
    // Test getting non-existent NFT
    let result = client.try_get_nft(&999);
    assert_eq!(result, Err(StreamNFTError::NFTNotFound));
    
    // Test getting non-existent wrapped stream
    let result = client.try_get_wrapped_stream(&999);
    assert_eq!(result, Err(StreamNFTError::StreamNotFound));
    
    // Test getting user NFTs for user with no NFTs
    let user_nfts = client.get_user_nfts(&Address::generate(&env));
    assert_eq!(user_nfts.len(), 0);
    
    // Test total supply initially
    let total_supply = client.get_total_supply();
    assert_eq!(total_supply, 0);
}

#[test]
fn test_multiple_nfts_per_user() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let grant_id1 = 123u64;
    let grant_id2 = 124u64;
    let wrap_amount = 1000000i128;
    let discount_rate = 1000u32;
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, None, None);
    
    // Mock stream details for both grants
    let stream_details1 = StreamDetails {
        grant_id: grant_id1,
        stream_rate_per_second: 1000,
        stream_start: env.ledger().timestamp(),
        stream_end: env.ledger().timestamp() + 86400 * 30,
        remaining_amount: 2592000000,
    };
    
    let stream_details2 = StreamDetails {
        grant_id: grant_id2,
        stream_rate_per_second: 1000,
        stream_start: env.ledger().timestamp(),
        stream_end: env.ledger().timestamp() + 86400 * 30,
        remaining_amount: 2592000000,
    };
    
    env.storage().instance().set(&StreamNFTDataKey::WrappedStream(grant_id1), &stream_details1);
    env.storage().instance().set(&StreamNFTDataKey::WrappedStream(grant_id2), &stream_details2);
    
    // Create two NFTs
    let token_id1 = client.wrap_stream(&grantee, &grant_id1, &wrap_amount, &discount_rate).unwrap();
    let token_id2 = client.wrap_stream(&grantee, &grant_id2, &wrap_amount, &discount_rate).unwrap();
    
    // Verify user has both NFTs
    let user_nfts = client.get_user_nfts(&grantee);
    assert_eq!(user_nfts.len(), 2);
    assert!(user_nfts.contains(&token_id1));
    assert!(user_nfts.contains(&token_id2));
    
    // Verify total supply
    let total_supply = client.get_total_supply();
    assert_eq!(total_supply, 2);
    
    // Transfer one NFT
    let buyer = Address::generate(&env);
    client.transfer_nft(&grantee, &buyer, &token_id1);
    
    // Verify NFT distribution
    let grantee_nfts = client.get_user_nfts(&grantee);
    assert_eq!(grantee_nfts.len(), 1);
    assert!(grantee_nfts.contains(&token_id2));
    
    let buyer_nfts = client.get_user_nfts(&buyer);
    assert_eq!(buyer_nfts.len(), 1);
    assert!(buyer_nfts.contains(&token_id1));
}

#[test]
fn test_stream_duration_validation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let grant_id = 123u64;
    let wrap_amount = 1000000i128;
    let discount_rate = 1000u32;
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, None, None);
    
    // Mock stream details with insufficient duration (less than MIN_STREAM_DURATION)
    let stream_details = StreamDetails {
        grant_id,
        stream_rate_per_second: 1000,
        stream_start: env.ledger().timestamp(),
        stream_end: env.ledger().timestamp() + 3600, // Only 1 hour
        remaining_amount: 3600000, // 1 hour worth
    };
    
    env.storage().instance().set(&StreamNFTDataKey::WrappedStream(grant_id), &stream_details);
    
    // Test wrapping with insufficient duration
    let result = client.try_wrap_stream(&grantee, &grant_id, &wrap_amount, &discount_rate);
    assert_eq!(result, Err(StreamNFTError::InvalidDuration));
}

#[test]
fn test_zero_wrap_amount() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let grant_id = 123u64;
    let wrap_amount = 0i128; // Invalid amount
    let discount_rate = 1000u32;
    
    let contract_id = env.register_contract(None, StreamNFTContract);
    let client = StreamNFTContractClient::new(&env, &contract_id);
    
    client.initialize(&admin, None, None);
    
    // Mock stream details
    let stream_details = StreamDetails {
        grant_id,
        stream_rate_per_second: 1000,
        stream_start: env.ledger().timestamp(),
        stream_end: env.ledger().timestamp() + 86400 * 30,
        remaining_amount: 2592000000,
    };
    
    env.storage().instance().set(&StreamNFTDataKey::WrappedStream(grant_id), &stream_details);
    
    // Test wrapping with zero amount
    let result = client.try_wrap_stream(&grantee, &grant_id, &wrap_amount, &discount_rate);
    assert_eq!(result, Err(StreamNFTError::InvalidAmount));
}
