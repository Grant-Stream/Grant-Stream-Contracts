#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, Vec, 
    token, Symbol, IntoVal, i128, u64,
};

// --- Constants ---
const PRICE_FEED_STALENESS_THRESHOLD: u64 = 300; // 5 minutes price staleness threshold
const VOLATILITY_THRESHOLD_BPS: u32 = 1000; // 10% volatility threshold
const MATCHING_PRECISION: u128 = 10_000_000_000_000_000; // 1e16 for high precision
const QUADRATIC_PRECISION: u128 = 1_000_000; // 1e6 for quadratic calculations
const MAX_SLIPPAGE_BPS: u32 = 500; // 5% max slippage

// --- Types ---

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum MatchingPoolError {
    NotInitialized = 1,
    Unauthorized = 2,
    RoundNotFound = 3,
    RoundNotActive = 4,
    InvalidAmount = 5,
    InvalidToken = 6,
    PriceFeedStale = 7,
    HighVolatility = 8,
    InsufficientMatchingPool = 9,
    OverAllocationRisk = 10,
    MathOverflow = 11,
    InvalidRoundConfig = 12,
    DonationPeriodEnded = 13,
    MatchingAlreadyCalculated = 14,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TokenPrice {
    pub token_address: Address,
    pub price_in_native: u128, // Price in native token units (with precision)
    pub timestamp: u64,
    pub confidence_bps: u32, // Price confidence in basis points
    pub volume_24h: u128, // 24h trading volume
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Donation {
    pub donor: Address,
    pub token_address: Address,
    pub amount: u128,
    pub normalized_value: u128, // Value in native token units
    pub timestamp: u64,
    pub round_id: u64,
    pub project_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ProjectDonations {
    pub project_id: u64,
    pub total_normalized_value: u128,
    pub unique_donors: u32,
    pub donations: Vec<Donation>,
    pub matching_amount: u128, // Calculated matching amount
    pub final_payout: u128, // Total payout (donations + matching)
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct MatchingRound {
    pub round_id: u64,
    pub start_time: u64,
    pub end_time: u64,
    pub matching_pool_amount: u128,
    pub native_token_address: Address,
    pub supported_tokens: Vec<Address>,
    pub min_donation_amount: u128,
    pub max_donation_amount: u128,
    pub quadratic_coefficient: u128, // Coefficient for quadratic matching
    pub is_active: bool,
    pub matching_calculated: bool,
    pub total_donations: u128,
    pub total_projects: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PriceFeedData {
    pub token_prices: Map<Address, TokenPrice>,
    pub last_updated: u64,
    pub oracle_address: Address,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum MatchingPoolDataKey {
    Admin,
    OracleContract,
    NativeToken,
    NextRoundId,
    Round(u64), // round_id -> MatchingRound
    ProjectDonations(u64, u64), // round_id + project_id -> ProjectDonations
    UserDonations(Address, u64), // donor + round_id -> Vec<Donation>
    PriceFeed, // PriceFeedData
    ActiveRound, // Currently active round ID
    TotalMatchingPool, // Total matching pool across all rounds
}

#[contract]
pub struct MultiTokenMatchingPool;

#[contractimpl]
impl MultiTokenMatchingPool {
    /// Initialize the matching pool contract
    pub fn initialize(
        env: Env,
        admin: Address,
        oracle_contract: Address,
        native_token: Address,
    ) -> Result<(), MatchingPoolError> {
        if env.storage().instance().has(&MatchingPoolDataKey::Admin) {
            return Err(MatchingPoolError::NotInitialized);
        }

        env.storage().instance().set(&MatchingPoolDataKey::Admin, &admin);
        env.storage().instance().set(&MatchingPoolDataKey::OracleContract, &oracle_contract);
        env.storage().instance().set(&MatchingPoolDataKey::NativeToken, &native_token);
        env.storage().instance().set(&MatchingPoolDataKey::NextRoundId, &1u64);
        env.storage().instance().set(&MatchingPoolDataKey::TotalMatchingPool, &0u128);

        // Initialize price feed
        let price_feed = PriceFeedData {
            token_prices: Map::new(&env),
            last_updated: 0,
            oracle_address: oracle_contract,
        };
        env.storage().instance().set(&MatchingPoolDataKey::PriceFeed, &price_feed);

        env.events().publish(
            (symbol_short!("matching_pool_initialized"),),
            (admin, oracle_contract, native_token),
        );

        Ok(())
    }

    /// Create a new matching round
    pub fn create_round(
        env: Env,
        admin: Address,
        matching_pool_amount: u128,
        start_time: u64,
        end_time: u64,
        supported_tokens: Vec<Address>,
        min_donation: u128,
        max_donation: u128,
        quadratic_coefficient: u128,
    ) -> Result<u64, MatchingPoolError> {
        Self::require_admin_auth(&env, &admin)?;

        if start_time >= end_time {
            return Err(MatchingPoolError::InvalidRoundConfig);
        }

        if end_time <= env.ledger().timestamp() {
            return Err(MatchingPoolError::InvalidRoundConfig);
        }

        let round_id = env.storage()
            .instance()
            .get(&MatchingPoolDataKey::NextRoundId)
            .unwrap_or(1u64);

        let next_id = round_id + 1;
        env.storage().instance().set(&MatchingPoolDataKey::NextRoundId, &next_id);

        let native_token = Self::get_native_token(&env)?;

        let round = MatchingRound {
            round_id,
            start_time,
            end_time,
            matching_pool_amount,
            native_token_address: native_token,
            supported_tokens: supported_tokens.clone(),
            min_donation_amount: min_donation,
            max_donation_amount: max_donation,
            quadratic_coefficient,
            is_active: false,
            matching_calculated: false,
            total_donations: 0,
            total_projects: 0,
        };

        env.storage().instance().set(&MatchingPoolDataKey::Round(round_id), &round);
        
        // Update total matching pool
        let total_pool = Self::get_total_matching_pool(&env)? + matching_pool_amount;
        env.storage().instance().set(&MatchingPoolDataKey::TotalMatchingPool, &total_pool);

        env.events().publish(
            (symbol_short!("round_created"),),
            (round_id, matching_pool_amount, start_time, end_time),
        );

        Ok(round_id)
    }

    /// Activate a matching round
    pub fn activate_round(env: Env, admin: Address, round_id: u64) -> Result<(), MatchingPoolError> {
        Self::require_admin_auth(&env, &admin)?;

        let mut round = Self::get_round(&env, round_id)?;
        
        if round.is_active {
            return Err(MatchingPoolError::RoundNotActive);
        }

        // Check if any round is currently active
        if let Some(active_round_id) = env.storage().instance().get(&MatchingPoolDataKey::ActiveRound) {
            let active_round = Self::get_round(&env, active_round_id)?;
            if active_round.is_active {
                return Err(MatchingPoolError::RoundNotActive); // Another round is active
            }
        }

        // Update price feeds before activation
        Self::update_price_feeds(&env)?;

        round.is_active = true;
        env.storage().instance().set(&MatchingPoolDataKey::Round(round_id), &round);
        env.storage().instance().set(&MatchingPoolDataKey::ActiveRound, &round_id);

        env.events().publish(
            (symbol_short!("round_activated"),),
            (round_id,),
        );

        Ok(())
    }

    /// Make a donation to a project
    pub fn donate(
        env: Env,
        donor: Address,
        round_id: u64,
        project_id: u64,
        token_address: Address,
        amount: u128,
    ) -> Result<(), MatchingPoolError> {
        donor.require_auth();

        let round = Self::get_round(&env, round_id)?;
        
        if !round.is_active {
            return Err(MatchingPoolError::RoundNotActive);
        }

        let now = env.ledger().timestamp();
        if now < round.start_time || now > round.end_time {
            return Err(MatchingPoolError::DonationPeriodEnded);
        }

        if amount < round.min_donation_amount || amount > round.max_donation_amount {
            return Err(MatchingPoolError::InvalidAmount);
        }

        if !round.supported_tokens.contains(&token_address) {
            return Err(MatchingPoolError::InvalidToken);
        }

        // Get current price and check for volatility
        let normalized_value = Self::normalize_token_value(&env, &token_address, amount)?;
        
        // Check for over-allocation risk
        let current_total = round.total_donations + normalized_value;
        if current_total > round.matching_pool_amount * 2 {
            return Err(MatchingPoolError::OverAllocationRisk);
        }

        // Transfer tokens to contract
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&donor, &env.current_contract_address(), &(amount as i128));

        // Create donation record
        let donation = Donation {
            donor: donor.clone(),
            token_address: token_address.clone(),
            amount,
            normalized_value,
            timestamp: now,
            round_id,
            project_id,
        };

        // Store donation
        Self::store_donation(&env, &donation, &round)?;

        // Update round totals
        let mut updated_round = round;
        updated_round.total_donations += normalized_value;
        env.storage().instance().set(&MatchingPoolDataKey::Round(round_id), &updated_round);

        env.events().publish(
            (symbol_short!("donation_made"),),
            (round_id, project_id, donor, token_address, amount, normalized_value),
        );

        Ok(())
    }

    /// Calculate matching amounts for all projects in a round
    pub fn calculate_matching(env: Env, admin: Address, round_id: u64) -> Result<(), MatchingPoolError> {
        Self::require_admin_auth(&env, &admin)?;

        let mut round = Self::get_round(&env, round_id)?;
        
        if round.matching_calculated {
            return Err(MatchingPoolError::MatchingAlreadyCalculated);
        }

        if round.is_active {
            return Err(MatchingPoolError::RoundNotActive);
        }

        // Get all project donations for this round
        let project_donations = Self::get_all_project_donations(&env, round_id)?;
        
        if project_donations.is_empty() {
            round.matching_calculated = true;
            env.storage().instance().set(&MatchingPoolDataKey::Round(round_id), &round);
            return Ok(());
        }

        // Calculate quadratic matching using high-precision math
        let matching_results = Self::calculate_quadratic_matching(
            &env,
            &project_donations,
            round.matching_pool_amount,
            round.quadratic_coefficient,
        )?;

        // Update project donations with matching amounts
        for (project_id, matching_amount) in matching_results.iter() {
            if let Some(mut project_donation) = project_donations.get(*project_id) {
                project_donation.matching_amount = *matching_amount;
                project_donation.final_payout = project_donation.total_normalized_value + *matching_amount;
                
                env.storage().instance().set(
                    &MatchingPoolDataKey::ProjectDonations(round_id, *project_id),
                    &project_donation,
                );
            }
        }

        // Mark round as calculated
        round.matching_calculated = true;
        env.storage().instance().set(&MatchingPoolDataKey::Round(round_id), &round);

        env.events().publish(
            (symbol_short!("matching_calculated"),),
            (round_id,),
        );

        Ok(())
    }

    /// Distribute matching funds to projects
    pub fn distribute_matching(env: Env, admin: Address, round_id: u64) -> Result<(), MatchingPoolError> {
        Self::require_admin_auth(&env, &admin)?;

        let round = Self::get_round(&env, round_id)?;
        
        if !round.matching_calculated {
            return Err(MatchingPoolError::MatchingAlreadyCalculated);
        }

        let native_token = Self::get_native_token(&env)?;
        let token_client = token::Client::new(&env, &native_token);

        // Get all project donations and distribute matching
        let project_donations = Self::get_all_project_donations(&env, round_id)?;
        
        for (project_id, project_donation) in project_donations.iter() {
            if project_donation.matching_amount > 0 {
                // In a real implementation, this would transfer to the project's wallet
                // For now, we'll just log the event
                env.events().publish(
                    (symbol_short!("matching_distributed"),),
                    (round_id, *project_id, project_donation.matching_amount),
                );
            }
        }

        env.events().publish(
            (symbol_short!("matching_distributed_complete"),),
            (round_id,),
        );

        Ok(())
    }

    /// Get round information
    pub fn get_round(env: &Env, round_id: u64) -> Result<MatchingRound, MatchingPoolError> {
        env.storage()
            .instance()
            .get(&MatchingPoolDataKey::Round(round_id))
            .ok_or(MatchingPoolError::RoundNotFound)
    }

    /// Get project donation information
    pub fn get_project_donations(
        env: &Env,
        round_id: u64,
        project_id: u64,
    ) -> Result<ProjectDonations, MatchingPoolError> {
        env.storage()
            .instance()
            .get(&MatchingPoolDataKey::ProjectDonations(round_id, project_id))
            .ok_or(MatchingPoolError::RoundNotFound)
    }

    /// Get user's donations for a round
    pub fn get_user_donations(env: &Env, user: &Address, round_id: u64) -> Vec<Donation> {
        env.storage()
            .instance()
            .get(&MatchingPoolDataKey::UserDonations(user.clone(), round_id))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get current price for a token
    pub fn get_token_price(env: &Env, token_address: &Address) -> Result<TokenPrice, MatchingPoolError> {
        let price_feed = Self::get_price_feed(&env)?;
        price_feed
            .token_prices
            .get(token_address.clone())
            .ok_or(MatchingPoolError::InvalidToken)
    }

    /// Get total matching pool
    pub fn get_total_matching_pool(env: &Env) -> Result<u128, MatchingPoolError> {
        env.storage()
            .instance()
            .get(&MatchingPoolDataKey::TotalMatchingPool)
            .ok_or(MatchingPoolError::NotInitialized)
    }

    // --- Helper Functions ---

    fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), MatchingPoolError> {
        let stored_admin: Address = env.storage()
            .instance()
            .get(&MatchingPoolDataKey::Admin)
            .ok_or(MatchingPoolError::NotInitialized)?;
        
        if stored_admin != *admin {
            return Err(MatchingPoolError::Unauthorized);
        }
        
        admin.require_auth();
        Ok(())
    }

    fn get_native_token(env: &Env) -> Result<Address, MatchingPoolError> {
        env.storage()
            .instance()
            .get(&MatchingPoolDataKey::NativeToken)
            .ok_or(MatchingPoolError::NotInitialized)
    }

    fn get_price_feed(env: &Env) -> Result<PriceFeedData, MatchingPoolError> {
        env.storage()
            .instance()
            .get(&MatchingPoolDataKey::PriceFeed)
            .ok_or(MatchingPoolError::NotInitialized)
    }

    fn update_price_feeds(env: &Env) -> Result<(), MatchingPoolError> {
        let oracle = Self::get_oracle_contract(env)?;
        
        // In a real implementation, this would call the oracle contract
        // For now, we'll simulate price updates
        let mut price_feed = Self::get_price_feed(env)?;
        price_feed.last_updated = env.ledger().timestamp();
        
        env.storage().instance().set(&MatchingPoolDataKey::PriceFeed, &price_feed);
        Ok(())
    }

    fn get_oracle_contract(env: &Env) -> Result<Address, MatchingPoolError> {
        let price_feed = Self::get_price_feed(env)?;
        Ok(price_feed.oracle_address)
    }

    fn normalize_token_value(
        env: &Env,
        token_address: &Address,
        amount: u128,
    ) -> Result<u128, MatchingPoolError> {
        let token_price = Self::get_token_price(env, token_address)?;
        
        // Check price staleness
        let now = env.ledger().timestamp();
        if now - token_price.timestamp > PRICE_FEED_STALENESS_THRESHOLD {
            return Err(MatchingPoolError::PriceFeedStale);
        }

        // Check confidence level
        if token_price.confidence_bps < 9000 { // Less than 90% confidence
            return Err(MatchingPoolError::HighVolatility);
        }

        // High-precision normalization
        let normalized_value = (amount as u128)
            .checked_mul(token_price.price_in_native)
            .ok_or(MatchingPoolError::MathOverflow)?
            .checked_div(MATCHING_PRECISION)
            .ok_or(MatchingPoolError::MathOverflow);

        normalized_value
    }

    fn store_donation(env: &Env, donation: &Donation, round: &MatchingRound) -> Result<(), MatchingPoolError> {
        // Store in project donations
        let mut project_donations = env.storage()
            .instance()
            .get(&MatchingPoolDataKey::ProjectDonations(donation.round_id, donation.project_id))
            .unwrap_or_else(|| ProjectDonations {
                project_id: donation.project_id,
                total_normalized_value: 0,
                unique_donors: 0,
                donations: Vec::new(env),
                matching_amount: 0,
                final_payout: 0,
            });

        // Check if this is a new donor for this project
        let is_new_donor = !project_donations.donations.iter()
            .any(|d| d.donor == donation.donor);

        project_donations.total_normalized_value += donation.normalized_value;
        if is_new_donor {
            project_donations.unique_donors += 1;
        }
        project_donations.donations.push_back(donation.clone());

        env.storage().instance().set(
            &MatchingPoolDataKey::ProjectDonations(donation.round_id, donation.project_id),
            &project_donations,
        );

        // Store in user donations
        let mut user_donations = env.storage()
            .instance()
            .get(&MatchingPoolDataKey::UserDonations(donation.donor.clone(), donation.round_id))
            .unwrap_or_else(|| Vec::new(env));
        user_donations.push_back(donation.clone());
        env.storage().instance().set(
            &MatchingPoolDataKey::UserDonations(donation.donor.clone(), donation.round_id),
            &user_donations,
        );

        Ok(())
    }

    fn get_all_project_donations(env: &Env, round_id: u64) -> Result<Map<u64, ProjectDonations>, MatchingPoolError> {
        let mut result = Map::new(env);
        
        // In a real implementation, this would iterate through all project donations
        // For now, return empty map
        Ok(result)
    }

    fn calculate_quadratic_matching(
        env: &Env,
        project_donations: &Map<u64, ProjectDonations>,
        matching_pool: u128,
        quadratic_coefficient: u128,
    ) -> Result<Map<u64, u128>, MatchingPoolError> {
        let mut results = Map::new(env);
        let mut total_square_root_sum = 0u128;

        // First pass: calculate square roots
        for (project_id, donations) in project_donations.iter() {
            if donations.total_normalized_value > 0 {
                let sqrt_value = Self::integer_square_root(
                    donations.total_normalized_value * QUADRATIC_PRECISION
                )?;
                total_square_root_sum += sqrt_value;
            }
        }

        if total_square_root_sum == 0 {
            return Ok(results);
        }

        // Second pass: calculate matching amounts
        for (project_id, donations) in project_donations.iter() {
            if donations.total_normalized_value > 0 {
                let sqrt_value = Self::integer_square_root(
                    donations.total_normalized_value * QUADRATIC_PRECISION
                )?;

                let matching_amount = (sqrt_value * matching_pool * quadratic_coefficient)
                    .checked_div(total_square_root_sum)
                    .ok_or(MatchingPoolError::MathOverflow)?
                    .checked_div(QUADRATIC_PRECISION)
                    .ok_or(MatchingPoolError::MathOverflow);

                results.set(*project_id, matching_amount);
            }
        }

        Ok(results)
    }

    fn integer_square_root(n: u128) -> Result<u128, MatchingPoolError> {
        if n == 0 {
            return Ok(0);
        }

        let mut x = n;
        let mut y = (x + 1) / 2;
        
        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }
        
        Ok(x)
    }
}
