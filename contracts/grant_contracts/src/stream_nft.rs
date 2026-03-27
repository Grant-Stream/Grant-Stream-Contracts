#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env,
    IntoVal, Map, Symbol, Vec,
};

// --- Constants ---
const MIN_STREAM_DURATION: u64 = 86400; // Minimum 1 day stream to be eligible for wrapping
const MAX_DISCOUNT_RATE: u32 = 5000; // Maximum 50% discount (5000 basis points)
const MARKETPLACE_FEE_BPS: u32 = 250; // 2.5% marketplace fee

// --- Types ---

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum StreamNFTError {
    NotInitialized = 1,
    Unauthorized = 2,
    StreamNotFound = 3,
    StreamAlreadyWrapped = 4,
    StreamNotWrapped = 5,
    InvalidAmount = 6,
    InvalidDuration = 7,
    DiscountRateTooHigh = 8,
    NFTNotFound = 9,
    TransferFailed = 10,
    InsufficientBalance = 11,
    StreamExpired = 12,
    InvalidRecipient = 13,
    MarketplaceNotSet = 14,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct StreamNFT {
    pub token_id: u64,
    pub original_grantee: Address,
    pub current_holder: Address,
    pub grant_id: u64,
    pub total_amount: i128,
    pub remaining_amount: i128,
    pub stream_start: u64,
    pub stream_end: u64,
    pub created_at: u64,
    pub discount_rate_bps: u32, // Discount rate when created
    pub marketplace_fee_paid: bool,
    pub is_active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct WrappedStream {
    pub nft_token_id: u64,
    pub original_beneficiary: Address,
    pub current_beneficiary: Address,
    pub wrapped_amount: i128,
    pub wrap_timestamp: u64,
    pub stream_rate_per_second: i128,
    pub last_claim_timestamp: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct MarketplaceListing {
    pub nft_token_id: u64,
    pub seller: Address,
    pub asking_price: i128,
    pub listed_at: u64,
    pub expires_at: Option<u64>,
    pub is_active: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum StreamNFTDataKey {
    Admin,
    NextTokenId,
    NFT(u64),                // token_id -> StreamNFT
    WrappedStream(u64),      // token_id -> WrappedStream
    OriginalStreamNFT(u64),  // grant_id -> nft_token_id
    UserNFTs(Address),       // user -> Vec<token_id>
    MarketplaceListing(u64), // token_id -> MarketplaceListing
    MarketplaceContract,
    ActiveListings, // Vec<token_id>
    TotalSupply,
    RoyaltyRecipient,
}

#[contract]
pub struct StreamNFTContract;

#[contractimpl]
impl StreamNFTContract {
    /// Initialize the Stream NFT contract
    pub fn initialize(
        env: Env,
        admin: Address,
        marketplace_contract: Option<Address>,
        royalty_recipient: Option<Address>,
    ) -> Result<(), StreamNFTError> {
        if env.storage().instance().has(&StreamNFTDataKey::Admin) {
            return Err(StreamNFTError::NotInitialized);
        }

        env.storage()
            .instance()
            .set(&StreamNFTDataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::NextTokenId, &1u64);
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::TotalSupply, &0u64);

        if let Some(marketplace) = marketplace_contract {
            env.storage()
                .instance()
                .set(&StreamNFTDataKey::MarketplaceContract, &marketplace);
        }

        if let Some(royalty) = royalty_recipient {
            env.storage()
                .instance()
                .set(&StreamNFTDataKey::RoyaltyRecipient, &royalty);
        }

        env.events()
            .publish((symbol_short!("stream_nft_initialized"),), (admin,));

        Ok(())
    }

    /// Wrap a grant stream into an NFT for instant liquidity
    pub fn wrap_stream(
        env: Env,
        grantee: Address,
        grant_id: u64,
        wrap_amount: i128,
        discount_rate_bps: u32,
    ) -> Result<u64, StreamNFTError> {
        grantee.require_auth();

        // Validate discount rate
        if discount_rate_bps > MAX_DISCOUNT_RATE {
            return Err(StreamNFTError::DiscountRateTooHigh);
        }

        // Get the grant stream details (this would interface with the main grant contract)
        let stream_details = Self::get_stream_details(&env, &grant_id)?;

        // Validate stream eligibility
        if stream_details.remaining_amount < wrap_amount {
            return Err(StreamNFTError::InsufficientBalance);
        }

        let stream_duration = stream_details.stream_end - env.ledger().timestamp();
        if stream_duration < MIN_STREAM_DURATION {
            return Err(StreamNFTError::InvalidDuration);
        }

        // Check if stream is already wrapped
        if env
            .storage()
            .instance()
            .has(&StreamNFTDataKey::OriginalStreamNFT(*grant_id))
        {
            return Err(StreamNFTError::StreamAlreadyWrapped);
        }

        // Create NFT
        let token_id = Self::mint_nft(
            &env,
            grantee.clone(),
            grant_id,
            wrap_amount,
            stream_details,
            discount_rate_bps,
        )?;

        // Update the original stream to redirect to NFT holder
        Self::update_stream_beneficiary(&env, grant_id, &env.current_contract_address())?;

        // Store wrapped stream details
        let wrapped_stream = WrappedStream {
            nft_token_id: token_id,
            original_beneficiary: grantee.clone(),
            current_beneficiary: grantee.clone(),
            wrapped_amount: wrap_amount,
            wrap_timestamp: env.ledger().timestamp(),
            stream_rate_per_second: stream_details.stream_rate_per_second,
            last_claim_timestamp: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&StreamNFTDataKey::WrappedStream(token_id), &wrapped_stream);
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::OriginalStreamNFT(*grant_id), &token_id);

        env.events().publish(
            (symbol_short!("stream_wrapped"),),
            (*grant_id, token_id, wrap_amount, discount_rate_bps),
        );

        Ok(token_id)
    }

    /// Unwrap a stream NFT and return to original beneficiary
    pub fn unwrap_stream(env: Env, holder: Address, token_id: u64) -> Result<(), StreamNFTError> {
        holder.require_auth();

        let nft = Self::get_nft(&env, token_id)?;
        if nft.current_holder != holder {
            return Err(StreamNFTError::Unauthorized);
        }

        if !nft.is_active {
            return Err(StreamNFTError::StreamExpired);
        }

        let wrapped_stream = Self::get_wrapped_stream(&env, token_id)?;

        // Calculate remaining amount and return to original beneficiary
        let remaining_amount = Self::calculate_remaining_stream_amount(&env, &wrapped_stream);

        // Update stream beneficiary back to original
        Self::update_stream_beneficiary(&env, nft.grant_id, &wrapped_stream.original_beneficiary)?;

        // Burn the NFT
        Self::burn_nft(&env, token_id)?;

        // Clean up storage
        env.storage()
            .instance()
            .remove(&StreamNFTDataKey::WrappedStream(token_id));
        env.storage()
            .instance()
            .remove(&StreamNFTDataKey::OriginalStreamNFT(nft.grant_id));

        env.events().publish(
            (symbol_short!("stream_unwrapped"),),
            (token_id, nft.grant_id, remaining_amount),
        );

        Ok(())
    }

    /// Transfer NFT to new holder
    pub fn transfer_nft(
        env: Env,
        from: Address,
        to: Address,
        token_id: u64,
    ) -> Result<(), StreamNFTError> {
        from.require_auth();

        let mut nft = Self::get_nft(&env, token_id)?;
        if nft.current_holder != from {
            return Err(StreamNFTError::Unauthorized);
        }

        if !nft.is_active {
            return Err(StreamNFTError::StreamExpired);
        }

        // Update NFT ownership
        nft.current_holder = to.clone();
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::NFT(token_id), &nft);

        // Update user NFT lists
        Self::remove_user_nft(&env, &from, token_id);
        Self::add_user_nft(&env, &to, token_id);

        // Update wrapped stream beneficiary
        let mut wrapped_stream = Self::get_wrapped_stream(&env, token_id)?;
        wrapped_stream.current_beneficiary = to.clone();
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::WrappedStream(token_id), &wrapped_stream);

        // Update the actual stream beneficiary
        Self::update_stream_beneficiary(&env, nft.grant_id, &to)?;

        env.events()
            .publish((symbol_short!("nft_transferred"),), (token_id, from, to));

        Ok(())
    }

    /// Claim available stream funds for NFT holder
    pub fn claim_stream_funds(
        env: Env,
        holder: Address,
        token_id: u64,
    ) -> Result<i128, StreamNFTError> {
        holder.require_auth();

        let nft = Self::get_nft(&env, token_id)?;
        if nft.current_holder != holder {
            return Err(StreamNFTError::Unauthorized);
        }

        let wrapped_stream = Self::get_wrapped_stream(&env, token_id)?;
        let available_amount = Self::calculate_available_stream_amount(&env, &wrapped_stream);

        if available_amount <= 0 {
            return Ok(0);
        }

        // Update last claim timestamp
        let mut updated_stream = wrapped_stream.clone();
        updated_stream.last_claim_timestamp = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::WrappedStream(token_id), &updated_stream);

        // Transfer funds to holder
        let token_client = token::Client::new(&env, &Self::get_grant_token_address(&env)?);
        token_client.transfer(&env.current_contract_address(), &holder, &available_amount);

        env.events().publish(
            (symbol_short!("stream_claimed"),),
            (token_id, holder, available_amount),
        );

        Ok(available_amount)
    }

    /// List NFT on marketplace
    pub fn list_on_marketplace(
        env: Env,
        seller: Address,
        token_id: u64,
        asking_price: i128,
        duration_seconds: Option<u64>,
    ) -> Result<(), StreamNFTError> {
        seller.require_auth();

        let nft = Self::get_nft(&env, token_id)?;
        if nft.current_holder != seller {
            return Err(StreamNFTError::Unauthorized);
        }

        let marketplace_contract = Self::get_marketplace_contract(&env)?;

        let listing = MarketplaceListing {
            nft_token_id: token_id,
            seller: seller.clone(),
            asking_price,
            listed_at: env.ledger().timestamp(),
            expires_at: duration_seconds.map(|d| env.ledger().timestamp() + d),
            is_active: true,
        };

        env.storage()
            .instance()
            .set(&StreamNFTDataKey::MarketplaceListing(token_id), &listing);

        // Add to active listings
        let mut active_listings = Self::get_active_listings(&env);
        active_listings.push_back(token_id);
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::ActiveListings, &active_listings);

        // Create marketplace order (this would call the marketplace contract)
        Self::create_marketplace_order(
            &env,
            &marketplace_contract,
            token_id,
            asking_price,
            duration_seconds,
        )?;

        env.events().publish(
            (symbol_short!("nft_listed"),),
            (token_id, seller, asking_price),
        );

        Ok(())
    }

    /// Get NFT details
    pub fn get_nft(env: &Env, token_id: u64) -> Result<StreamNFT, StreamNFTError> {
        env.storage()
            .instance()
            .get(&StreamNFTDataKey::NFT(token_id))
            .ok_or(StreamNFTError::NFTNotFound)
    }

    /// Get wrapped stream details
    pub fn get_wrapped_stream(env: &Env, token_id: u64) -> Result<WrappedStream, StreamNFTError> {
        env.storage()
            .instance()
            .get(&StreamNFTDataKey::WrappedStream(token_id))
            .ok_or(StreamNFTError::StreamNotFound)
    }

    /// Get user's NFTs
    pub fn get_user_nfts(env: &Env, user: &Address) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&StreamNFTDataKey::UserNFTs(user.clone()))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get total supply
    pub fn get_total_supply(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&StreamNFTDataKey::TotalSupply)
            .unwrap_or(0)
    }

    // --- Helper Functions ---

    fn mint_nft(
        env: &Env,
        grantee: Address,
        grant_id: u64,
        wrap_amount: i128,
        stream_details: StreamDetails,
        discount_rate_bps: u32,
    ) -> Result<u64, StreamNFTError> {
        let token_id = env
            .storage()
            .instance()
            .get(&StreamNFTDataKey::NextTokenId)
            .unwrap_or(1u64);

        let next_id = token_id + 1;
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::NextTokenId, &next_id);

        let total_supply = Self::get_total_supply(env) + 1;
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::TotalSupply, &total_supply);

        let nft = StreamNFT {
            token_id,
            original_grantee: grantee.clone(),
            current_holder: grantee.clone(),
            grant_id,
            total_amount: wrap_amount,
            remaining_amount: wrap_amount,
            stream_start: stream_details.stream_start,
            stream_end: stream_details.stream_end,
            created_at: env.ledger().timestamp(),
            discount_rate_bps,
            marketplace_fee_paid: false,
            is_active: true,
        };

        env.storage()
            .instance()
            .set(&StreamNFTDataKey::NFT(token_id), &nft);
        Self::add_user_nft(env, &grantee, token_id);

        Ok(token_id)
    }

    fn burn_nft(env: &Env, token_id: u64) -> Result<(), StreamNFTError> {
        let nft = Self::get_nft(env, token_id)?;

        Self::remove_user_nft(env, &nft.current_holder, token_id);

        let total_supply = Self::get_total_supply(env) - 1;
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::TotalSupply, &total_supply);

        env.storage()
            .instance()
            .remove(&StreamNFTDataKey::NFT(token_id));

        Ok(())
    }

    fn add_user_nft(env: &Env, user: &Address, token_id: u64) {
        let mut user_nfts = Self::get_user_nfts(env, user);
        user_nfts.push_back(token_id);
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::UserNFTs(user.clone()), &user_nfts);
    }

    fn remove_user_nft(env: &Env, user: &Address, token_id: u64) {
        let mut user_nfts = Self::get_user_nfts(env, user);
        let new_list: Vec<u64> = user_nfts.iter().filter(|&id| id != token_id).collect();
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::UserNFTs(user.clone()), &new_list);
    }

    fn calculate_available_stream_amount(env: &Env, wrapped_stream: &WrappedStream) -> i128 {
        let now = env.ledger().timestamp();
        let time_elapsed = now - wrapped_stream.last_claim_timestamp;

        if time_elapsed <= 0
            || now
                >= wrapped_stream.wrap_timestamp
                    + (wrapped_stream.wrapped_amount / wrapped_stream.stream_rate_per_second) as u64
        {
            return 0;
        }

        let accrued = wrapped_stream.stream_rate_per_second * time_elapsed as i128;
        let remaining_wrapped = Self::calculate_remaining_stream_amount(env, wrapped_stream);

        accrued.min(remaining_wrapped)
    }

    fn calculate_remaining_stream_amount(env: &Env, wrapped_stream: &WrappedStream) -> i128 {
        let now = env.ledger().timestamp();
        let total_duration = wrapped_stream.wrapped_amount / wrapped_stream.stream_rate_per_second;
        let elapsed = now - wrapped_stream.wrap_timestamp;

        if elapsed >= total_duration as u64 {
            return 0;
        }

        let accrued = wrapped_stream.stream_rate_per_second * elapsed as i128;
        wrapped_stream.wrapped_amount - accrued
    }

    fn get_stream_details(env: &Env, grant_id: u64) -> Result<StreamDetails, StreamNFTError> {
        // This would interface with the main grant contract to get stream details
        // For now, returning a mock implementation
        Ok(StreamDetails {
            grant_id,
            stream_rate_per_second: 1000, // 1000 tokens per second
            stream_start: env.ledger().timestamp(),
            stream_end: env.ledger().timestamp() + 86400 * 30, // 30 days
            remaining_amount: 2592000000,                      // Total amount for 30 days
        })
    }

    fn update_stream_beneficiary(
        env: &Env,
        grant_id: u64,
        new_beneficiary: &Address,
    ) -> Result<(), StreamNFTError> {
        // This would call the main grant contract to update the beneficiary
        // For now, we'll just store the mapping
        env.storage()
            .instance()
            .set(&StreamNFTDataKey::WrappedStream(grant_id), new_beneficiary);
        Ok(())
    }

    fn get_marketplace_contract(env: &Env) -> Result<Address, StreamNFTError> {
        env.storage()
            .instance()
            .get(&StreamNFTDataKey::MarketplaceContract)
            .ok_or(StreamNFTError::MarketplaceNotSet)
    }

    fn get_grant_token_address(env: &Env) -> Result<Address, StreamNFTError> {
        // This would get the token address from the main grant contract
        // For now, returning a mock address
        Ok(Address::generate(env))
    }

    fn get_active_listings(env: &Env) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&StreamNFTDataKey::ActiveListings)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn create_marketplace_order(
        env: &Env,
        marketplace: &Address,
        token_id: u64,
        price: i128,
        duration: Option<u64>,
    ) -> Result<(), StreamNFTError> {
        // This would create an order on the marketplace contract
        // For now, just logging the event
        env.events().publish(
            (symbol_short!("marketplace_order_created"),),
            (token_id, price, duration.unwrap_or(0)),
        );
        Ok(())
    }
}

// --- Supporting Types ---

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamDetails {
    pub grant_id: u64,
    pub stream_rate_per_second: i128,
    pub stream_start: u64,
    pub stream_end: u64,
    pub remaining_amount: i128,
}
