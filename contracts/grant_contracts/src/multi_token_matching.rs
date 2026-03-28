#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, i128, symbol_short, token, u64, Address,
    Env, IntoVal, Map, Symbol, Vec,
};

// --- Constants ---
const PRICE_FEED_STALENESS_THRESHOLD: u64 = 300; // 5 minutes price staleness threshold
const VOLATILITY_THRESHOLD_BPS: u32 = 1000; // 10% volatility threshold
const MATCHING_PRECISION: u128 = 10_000_000_000_000_000; // 1e16 for high precision
const QUADRATIC_PRECISION: u128 = 1_000_000; // 1e6 for quadratic calculations
const MAX_SLIPPAGE_BPS: u32 = 500; // 5% max slippage

// --- Slippage Protection Constants ---
const DEFAULT_SLIPPAGE_THRESHOLD_BPS: u32 = 100; // 1% default slippage threshold
const DEX_QUERY_TIMEOUT_SECS: u64 = 30; // 30 seconds timeout for DEX queries
const MIN_SPREAD_CONFIDENCE_BPS: u32 = 8000; // 80% minimum confidence for spread data
const SWAP_QUEUE_MAX_SIZE: u32 = 1000; // Maximum queued swaps
const SWAP_QUEUE_EXPIRY_SECS: u64 = 3600; // 1 hour expiry for queued swaps

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
    // Slippage Protection Errors
    DexQueryFailed = 15,
    SlippageExceedsThreshold = 16,
    SwapQueueFull = 17,
    InsufficientLiquidity = 18,
    SpreadDataStale = 19,
    InvalidSwapRequest = 20,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TokenPrice {
    pub token_address: Address,
    pub price_in_native: u128, // Price in native token units (with precision)
    pub timestamp: u64,
    pub confidence_bps: u32, // Price confidence in basis points
    pub volume_24h: u128,    // 24h trading volume
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
    pub final_payout: u128,    // Total payout (donations + matching)
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

// --- Slippage Protection Types ---

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct DexSpread {
    pub token_a: Address,
    pub token_b: Address,
    pub bid_price: u128, // Price of token_a in terms of token_b (buying token_a)
    pub ask_price: u128, // Price of token_a in terms of token_b (selling token_a)
    pub spread_bps: u32, // Spread in basis points
    pub liquidity_depth: u128, // Available liquidity at current spread
    pub timestamp: u64,
    pub confidence_bps: u32, // Confidence in spread data
    pub dex_source: String,  // DEX identifier
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct SlippageConfig {
    pub max_slippage_bps: u32,    // Maximum allowed slippage in basis points
    pub auto_queue_enabled: bool, // Whether to auto-queue high-slippage swaps
    pub min_liquidity_threshold: u128, // Minimum liquidity required
    pub spread_confidence_threshold: u32, // Minimum confidence for spread data
    pub queue_expiry_secs: u64,   // How long queued swaps remain valid
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct QueuedSwap {
    pub swap_id: u64,
    pub round_id: u64,
    pub from_token: Address,
    pub to_token: Address,
    pub amount: u128,
    pub min_received: u128, // Minimum amount to receive
    pub queued_at: u64,
    pub expires_at: u64,
    pub priority: u32,    // Priority for execution (lower = higher priority)
    pub retry_count: u32, // Number of retry attempts
    pub max_retries: u32, // Maximum retry attempts
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct SlippageGuard {
    pub config: SlippageConfig,
    pub active_swaps: Map<u64, QueuedSwap>, // Active queued swaps
    pub swap_queue: Vec<u64>,               // Queue of swap IDs
    pub next_swap_id: u64,
    pub total_queued_amount: u128,
    pub last_dex_query: u64,
    pub dex_query_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum MatchingPoolDataKey {
    Admin,
    OracleContract,
    NativeToken,
    NextRoundId,
    Round(u64),                  // round_id -> MatchingRound
    ProjectDonations(u64, u64),  // round_id + project_id -> ProjectDonations
    UserDonations(Address, u64), // donor + round_id -> Vec<Donation>
    PriceFeed,                   // PriceFeedData
    ActiveRound,                 // Currently active round ID
    TotalMatchingPool,           // Total matching pool across all rounds
    // Slippage Protection Keys
    SlippageGuard,   // SlippageGuard configuration and state
    QueuedSwap(u64), // swap_id -> QueuedSwap
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

        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::OracleContract, &oracle_contract);
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::NativeToken, &native_token);
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::NextRoundId, &1u64);
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::TotalMatchingPool, &0u128);

        // Initialize price feed
        let price_feed = PriceFeedData {
            token_prices: Map::new(&env),
            last_updated: 0,
            oracle_address: oracle_contract,
        };
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::PriceFeed, &price_feed);

        // Initialize slippage guard with default configuration
        let slippage_config = SlippageConfig {
            max_slippage_bps: DEFAULT_SLIPPAGE_THRESHOLD_BPS,
            auto_queue_enabled: true,
            min_liquidity_threshold: 1000u128, // Minimum liquidity threshold
            spread_confidence_threshold: MIN_SPREAD_CONFIDENCE_BPS,
            queue_expiry_secs: SWAP_QUEUE_EXPIRY_SECS,
        };

        let slippage_guard = SlippageGuard {
            config: slippage_config,
            active_swaps: Map::new(&env),
            swap_queue: Vec::new(&env),
            next_swap_id: 1,
            total_queued_amount: 0,
            last_dex_query: 0,
            dex_query_count: 0,
        };
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::SlippageGuard, &slippage_guard);

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

        let round_id = env
            .storage()
            .instance()
            .get(&MatchingPoolDataKey::NextRoundId)
            .unwrap_or(1u64);

        let next_id = round_id + 1;
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::NextRoundId, &next_id);

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

        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::Round(round_id), &round);

        // Update total matching pool
        let total_pool = Self::get_total_matching_pool(&env)? + matching_pool_amount;
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::TotalMatchingPool, &total_pool);

        env.events().publish(
            (symbol_short!("round_created"),),
            (round_id, matching_pool_amount, start_time, end_time),
        );

        Ok(round_id)
    }

    /// Activate a matching round
    pub fn activate_round(
        env: Env,
        admin: Address,
        round_id: u64,
    ) -> Result<(), MatchingPoolError> {
        Self::require_admin_auth(&env, &admin)?;

        let mut round = Self::get_round(&env, round_id)?;

        if round.is_active {
            return Err(MatchingPoolError::RoundNotActive);
        }

        // Check if any round is currently active
        if let Some(active_round_id) = env
            .storage()
            .instance()
            .get(&MatchingPoolDataKey::ActiveRound)
        {
            let active_round = Self::get_round(&env, active_round_id)?;
            if active_round.is_active {
                return Err(MatchingPoolError::RoundNotActive); // Another round is active
            }
        }

        // Update price feeds before activation
        Self::update_price_feeds(&env)?;

        round.is_active = true;
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::Round(round_id), &round);
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::ActiveRound, &round_id);

        env.events()
            .publish((symbol_short!("round_activated"),), (round_id,));

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
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::Round(round_id), &updated_round);

        env.events().publish(
            (symbol_short!("donation_made"),),
            (
                round_id,
                project_id,
                donor,
                token_address,
                amount,
                normalized_value,
            ),
        );

        Ok(())
    }

    /// Calculate matching amounts for all projects in a round
    pub fn calculate_matching(
        env: Env,
        admin: Address,
        round_id: u64,
    ) -> Result<(), MatchingPoolError> {
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
            env.storage()
                .instance()
                .set(&MatchingPoolDataKey::Round(round_id), &round);
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
                project_donation.final_payout =
                    project_donation.total_normalized_value + *matching_amount;

                env.storage().instance().set(
                    &MatchingPoolDataKey::ProjectDonations(round_id, *project_id),
                    &project_donation,
                );
            }
        }

        // Mark round as calculated
        round.matching_calculated = true;
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::Round(round_id), &round);

        env.events()
            .publish((symbol_short!("matching_calculated"),), (round_id,));

        Ok(())
    }

    /// Distribute matching funds to projects
    pub fn distribute_matching(
        env: Env,
        admin: Address,
        round_id: u64,
    ) -> Result<(), MatchingPoolError> {
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

    // --- Slippage Protection Functions ---

    /// Configure slippage protection settings (DAO admin only)
    pub fn configure_slippage_protection(
        env: Env,
        admin: Address,
        max_slippage_bps: u32,
        auto_queue_enabled: bool,
        min_liquidity_threshold: u128,
        spread_confidence_threshold: u32,
        queue_expiry_secs: u64,
    ) -> Result<(), MatchingPoolError> {
        Self::require_admin_auth(&env, &admin)?;

        let mut slippage_guard = Self::get_slippage_guard(&env)?;

        slippage_guard.config.max_slippage_bps = max_slippage_bps;
        slippage_guard.config.auto_queue_enabled = auto_queue_enabled;
        slippage_guard.config.min_liquidity_threshold = min_liquidity_threshold;
        slippage_guard.config.spread_confidence_threshold = spread_confidence_threshold;
        slippage_guard.config.queue_expiry_secs = queue_expiry_secs;

        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::SlippageGuard, &slippage_guard);

        env.events().publish(
            (symbol_short!("slippage_config_updated"),),
            (
                max_slippage_bps,
                auto_queue_enabled,
                min_liquidity_threshold,
            ),
        );

        Ok(())
    }

    /// Query current DEX spread for a token pair
    pub fn query_dex_spread(
        env: Env,
        token_a: Address,
        token_b: Address,
    ) -> Result<DexSpread, MatchingPoolError> {
        let now = env.ledger().timestamp();

        // Update DEX query statistics
        let mut slippage_guard = Self::get_slippage_guard(&env)?;
        slippage_guard.last_dex_query = now;
        slippage_guard.dex_query_count += 1;
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::SlippageGuard, &slippage_guard);

        // In a real implementation, this would query actual Stellar DEX data
        // For now, we simulate the spread calculation
        let spread = Self::simulate_dex_spread(&env, &token_a, &token_b)?;

        env.events().publish(
            (symbol_short!("dex_spread_queried"),),
            (token_a, token_b, spread.spread_bps, spread.liquidity_depth),
        );

        Ok(spread)
    }

    /// Execute token swap with slippage protection
    pub fn execute_swap_with_protection(
        env: Env,
        round_id: u64,
        from_token: Address,
        to_token: Address,
        amount: u128,
        min_received: u128,
    ) -> Result<u128, MatchingPoolError> {
        let round = Self::get_round(&env, round_id)?;
        if !round.is_active {
            return Err(MatchingPoolError::RoundNotActive);
        }

        // Get current DEX spread
        let spread = Self::query_dex_spread(&env, from_token.clone(), to_token.clone())?;

        // Check if spread data is fresh enough
        let now = env.ledger().timestamp();
        if now - spread.timestamp > DEX_QUERY_TIMEOUT_SECS {
            return Err(MatchingPoolError::SpreadDataStale);
        }

        // Check confidence level
        let slippage_guard = Self::get_slippage_guard(&env)?;
        if spread.confidence_bps < slippage_guard.config.spread_confidence_threshold {
            return Err(MatchingPoolError::DexQueryFailed);
        }

        // Calculate expected output and slippage
        let expected_output = Self::calculate_swap_output(&env, amount, &spread)?;
        let slippage_bps = Self::calculate_slippage_bps(amount, expected_output, min_received)?;

        if slippage_bps > slippage_guard.config.max_slippage_bps {
            // Slippage exceeds threshold - queue the swap if enabled
            if slippage_guard.config.auto_queue_enabled {
                return Self::queue_swap(
                    &env,
                    round_id,
                    from_token,
                    to_token,
                    amount,
                    min_received,
                );
            } else {
                return Err(MatchingPoolError::SlippageExceedsThreshold);
            }
        }

        // Check liquidity
        if spread.liquidity_depth < slippage_guard.config.min_liquidity_threshold {
            return Err(MatchingPoolError::InsufficientLiquidity);
        }

        // Execute the swap
        let actual_output =
            Self::execute_dex_swap(&env, from_token, to_token, amount, min_received)?;

        env.events().publish(
            (symbol_short!("swap_executed"),),
            (round_id, amount, actual_output, slippage_bps),
        );

        Ok(actual_output)
    }

    /// Queue a swap for later execution when slippage improves
    pub fn queue_swap(
        env: &Env,
        round_id: u64,
        from_token: Address,
        to_token: Address,
        amount: u128,
        min_received: u128,
    ) -> Result<u64, MatchingPoolError> {
        let mut slippage_guard = Self::get_slippage_guard(env)?;

        // Check queue capacity
        if slippage_guard.swap_queue.len() >= SWAP_QUEUE_MAX_SIZE {
            return Err(MatchingPoolError::SwapQueueFull);
        }

        let swap_id = slippage_guard.next_swap_id;
        let now = env.ledger().timestamp();
        let expires_at = now + slippage_guard.config.queue_expiry_secs;

        let queued_swap = QueuedSwap {
            swap_id,
            round_id,
            from_token,
            to_token,
            amount,
            min_received,
            queued_at: now,
            expires_at,
            priority: 1, // Default priority
            retry_count: 0,
            max_retries: 3, // Default max retries
        };

        // Add to active swaps and queue
        slippage_guard
            .active_swaps
            .set(swap_id, queued_swap.clone());
        slippage_guard.swap_queue.push_back(swap_id);
        slippage_guard.next_swap_id += 1;
        slippage_guard.total_queued_amount += amount;

        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::SlippageGuard, &slippage_guard);
        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::QueuedSwap(swap_id), &queued_swap);

        env.events().publish(
            (symbol_short!("swap_queued"),),
            (swap_id, round_id, amount, expires_at),
        );

        Ok(swap_id)
    }

    /// Process queued swaps when market conditions improve
    pub fn process_queued_swaps(env: Env, admin: Address) -> Result<Vec<u64>, MatchingPoolError> {
        Self::require_admin_auth(&env, &admin)?;

        let mut slippage_guard = Self::get_slippage_guard(&env)?;
        let mut processed_swaps = Vec::new(&env);
        let now = env.ledger().timestamp();

        // Process swaps in priority order
        let mut i = 0;
        while i < slippage_guard.swap_queue.len() {
            let swap_id = slippage_guard.swap_queue.get(i).unwrap();

            if let Some(queued_swap) = slippage_guard.active_swaps.get(swap_id) {
                // Check if swap has expired
                if now > queued_swap.expires_at {
                    // Remove expired swap
                    slippage_guard.active_swaps.remove(swap_id);
                    slippage_guard.swap_queue.remove(i);
                    slippage_guard.total_queued_amount -= queued_swap.amount;
                    env.storage()
                        .instance()
                        .remove(&MatchingPoolDataKey::QueuedSwap(swap_id));

                    env.events().publish(
                        (symbol_short!("swap_expired"),),
                        (swap_id, queued_swap.round_id),
                    );
                    continue;
                }

                // Try to execute the swap
                match Self::execute_swap_with_protection(
                    env.clone(),
                    queued_swap.round_id,
                    queued_swap.from_token.clone(),
                    queued_swap.to_token.clone(),
                    queued_swap.amount,
                    queued_swap.min_received,
                ) {
                    Ok(_) => {
                        // Swap successful - remove from queue
                        slippage_guard.active_swaps.remove(swap_id);
                        slippage_guard.swap_queue.remove(i);
                        slippage_guard.total_queued_amount -= queued_swap.amount;
                        env.storage()
                            .instance()
                            .remove(&MatchingPoolDataKey::QueuedSwap(swap_id));

                        processed_swaps.push_back(swap_id);

                        env.events().publish(
                            (symbol_short!("queued_swap_executed"),),
                            (swap_id, queued_swap.round_id),
                        );
                        continue;
                    }
                    Err(MatchingPoolError::SlippageExceedsThreshold) => {
                        // Still high slippage - increment retry count
                        let mut updated_swap = queued_swap;
                        updated_swap.retry_count += 1;

                        if updated_swap.retry_count >= updated_swap.max_retries {
                            // Max retries reached - remove from queue
                            slippage_guard.active_swaps.remove(swap_id);
                            slippage_guard.swap_queue.remove(i);
                            slippage_guard.total_queued_amount -= queued_swap.amount;
                            env.storage()
                                .instance()
                                .remove(&MatchingPoolDataKey::QueuedSwap(swap_id));

                            env.events().publish(
                                (symbol_short!("swap_failed_max_retries"),),
                                (swap_id, queued_swap.round_id),
                            );
                            continue;
                        } else {
                            // Update retry count
                            slippage_guard.active_swaps.set(swap_id, updated_swap);
                        }
                    }
                    Err(_) => {
                        // Other error - skip for now
                        i += 1;
                    }
                }
            } else {
                // Swap not found - remove from queue
                slippage_guard.swap_queue.remove(i);
            }

            i += 1;
        }

        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::SlippageGuard, &slippage_guard);

        env.events().publish(
            (symbol_short!("queued_swaps_processed"),),
            (processed_swaps.len(), slippage_guard.swap_queue.len()),
        );

        Ok(processed_swaps)
    }

    /// Get current slippage guard configuration
    pub fn get_slippage_config(env: &Env) -> Result<SlippageConfig, MatchingPoolError> {
        let slippage_guard = Self::get_slippage_guard(env)?;
        Ok(slippage_guard.config)
    }

    /// Get queued swaps for a round
    pub fn get_queued_swaps(
        env: &Env,
        round_id: u64,
    ) -> Result<Vec<QueuedSwap>, MatchingPoolError> {
        let slippage_guard = Self::get_slippage_guard(env)?;
        let mut round_swaps = Vec::new(env);

        for swap_id in slippage_guard.swap_queue.iter() {
            if let Some(queued_swap) = slippage_guard.active_swaps.get(swap_id) {
                if queued_swap.round_id == round_id {
                    round_swaps.push_back(queued_swap);
                }
            }
        }

        Ok(round_swaps)
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
    pub fn get_token_price(
        env: &Env,
        token_address: &Address,
    ) -> Result<TokenPrice, MatchingPoolError> {
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
        let stored_admin: Address = env
            .storage()
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

        env.storage()
            .instance()
            .set(&MatchingPoolDataKey::PriceFeed, &price_feed);
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
        if token_price.confidence_bps < 9000 {
            // Less than 90% confidence
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

    fn store_donation(
        env: &Env,
        donation: &Donation,
        round: &MatchingRound,
    ) -> Result<(), MatchingPoolError> {
        // Store in project donations
        let mut project_donations = env
            .storage()
            .instance()
            .get(&MatchingPoolDataKey::ProjectDonations(
                donation.round_id,
                donation.project_id,
            ))
            .unwrap_or_else(|| ProjectDonations {
                project_id: donation.project_id,
                total_normalized_value: 0,
                unique_donors: 0,
                donations: Vec::new(env),
                matching_amount: 0,
                final_payout: 0,
            });

        // Check if this is a new donor for this project
        let is_new_donor = !project_donations
            .donations
            .iter()
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
        let mut user_donations = env
            .storage()
            .instance()
            .get(&MatchingPoolDataKey::UserDonations(
                donation.donor.clone(),
                donation.round_id,
            ))
            .unwrap_or_else(|| Vec::new(env));
        user_donations.push_back(donation.clone());
        env.storage().instance().set(
            &MatchingPoolDataKey::UserDonations(donation.donor.clone(), donation.round_id),
            &user_donations,
        );

        Ok(())
    }

    fn get_all_project_donations(
        env: &Env,
        round_id: u64,
    ) -> Result<Map<u64, ProjectDonations>, MatchingPoolError> {
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
                    donations.total_normalized_value * QUADRATIC_PRECISION,
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
                    donations.total_normalized_value * QUADRATIC_PRECISION,
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

    // --- Slippage Protection Helper Functions ---

    fn get_slippage_guard(env: &Env) -> Result<SlippageGuard, MatchingPoolError> {
        env.storage()
            .instance()
            .get(&MatchingPoolDataKey::SlippageGuard)
            .ok_or(MatchingPoolError::NotInitialized)
    }

    fn simulate_dex_spread(
        env: &Env,
        token_a: &Address,
        token_b: &Address,
    ) -> Result<DexSpread, MatchingPoolError> {
        let now = env.ledger().timestamp();

        // Simulate spread calculation based on token addresses
        // In a real implementation, this would query actual Stellar DEX data
        let base_spread_bps = 50u32; // 0.5% base spread
        let liquidity_depth = 1_000_000u128; // Simulated liquidity
        let confidence_bps = 9500u32; // 95% confidence

        // Generate deterministic but realistic bid/ask prices
        let seed = token_a
            .clone()
            .contract_id()
            .iter()
            .chain(token_b.clone().contract_id().iter())
            .fold(0u64, |acc, &byte| {
                acc.wrapping_mul(31).wrapping_add(byte as u64)
            });
        let spread_variation = (seed % 100) as u32; // 0-99 bps variation
        let final_spread_bps = base_spread_bps + spread_variation;

        // Simulate bid and ask prices
        let mid_price = MATCHING_PRECISION; // Use matching precision as base
        let spread_amount = (mid_price * final_spread_bps as u128) / 10000;
        let bid_price = mid_price - spread_amount;
        let ask_price = mid_price + spread_amount;

        Ok(DexSpread {
            token_a: token_a.clone(),
            token_b: token_b.clone(),
            bid_price,
            ask_price,
            spread_bps: final_spread_bps,
            liquidity_depth,
            timestamp: now,
            confidence_bps,
            dex_source: String::from_str(env, "stellar_dex_v1"),
        })
    }

    fn calculate_swap_output(
        env: &Env,
        amount: u128,
        spread: &DexSpread,
    ) -> Result<u128, MatchingPoolError> {
        // Calculate output using ask price (selling token_a for token_b)
        let output = (amount * spread.ask_price) / MATCHING_PRECISION;

        // Apply slippage based on amount relative to liquidity
        let slippage_factor = if amount > spread.liquidity_depth / 10 {
            // Large trade: additional slippage
            9800u128 // 2% additional slippage
        } else {
            10000u128 // No additional slippage
        };

        let final_output = (output * slippage_factor) / 10000;
        Ok(final_output)
    }

    fn calculate_slippage_bps(
        input_amount: u128,
        expected_output: u128,
        min_received: u128,
    ) -> Result<u32, MatchingPoolError> {
        if expected_output == 0 {
            return Err(MatchingPoolError::InvalidAmount);
        }

        // Calculate slippage as percentage of expected output
        let slippage_amount = expected_output.saturating_sub(min_received);
        let slippage_bps = (slippage_amount * 10000) / expected_output;

        // Ensure we don't overflow u32
        Ok(slippage_bps.try_into().unwrap_or(10000))
    }

    fn execute_dex_swap(
        env: &Env,
        from_token: Address,
        to_token: Address,
        amount: u128,
        min_received: u128,
    ) -> Result<u128, MatchingPoolError> {
        // In a real implementation, this would execute the actual DEX swap
        // For now, we simulate the swap with a small, realistic slippage

        // Get current spread to calculate realistic output
        let spread = Self::simulate_dex_spread(env, &from_token, &to_token)?;
        let actual_output = Self::calculate_swap_output(env, amount, &spread)?;

        // Ensure we meet the minimum requirement
        if actual_output < min_received {
            return Err(MatchingPoolError::SlippageExceedsThreshold);
        }

        // Simulate token transfer (in reality, this would interact with Stellar DEX)
        let contract_address = env.current_contract_address();

        // Transfer from_token to contract (if not already there)
        let from_token_client = token::Client::new(env, &from_token);
        let contract_balance = from_token_client.balance(&contract_address);

        // For simulation, assume we have enough tokens
        if contract_balance < amount as i128 {
            // In reality, this would be handled differently
            return Err(MatchingPoolError::InsufficientLiquidity);
        }

        // Simulate receiving to_token (in reality, this would come from DEX)
        let to_token_client = token::Client::new(env, &to_token);

        // For testing purposes, we'll just log the swap
        env.logs().add(&format!(
            "DEX Swap: {} {} -> {} {} (rate: {})",
            amount, from_token, actual_output, to_token, spread.spread_bps
        ));

        Ok(actual_output)
    }
}

#[cfg(test)]
mod test_slippage_protection;
