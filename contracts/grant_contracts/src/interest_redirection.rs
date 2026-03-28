#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env,
    IntoVal, Map, String, Symbol, Token, TryFromVal, TryIntoVal, Vec,
};

// --- Interest Redirection Constants ---

pub const INTEREST_REDIRECTION_VERSION: u32 = 1;
pub const DEAD_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
pub const DEFAULT_BURN_RATIO: u32 = 5000; // 50% of yield redirected to burn
pub const MIN_BURN_RATIO: u32 = 1000; // 10% minimum burn ratio
pub const MAX_BURN_RATIO: u32 = 9000; // 90% maximum burn ratio
pub const BURN_EXECUTION_INTERVAL: u64 = 7 * 24 * 60 * 60; // 7 days
pub const MIN_YIELD_THRESHOLD: u128 = 1000; // Minimum yield to trigger burn
pub const AUTO_BURN_ENABLED: bool = true;

// --- Interest Redirection Types ---

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BurnConfig {
    pub admin: Address,
    pub burn_ratio: u32,           // Percentage of yield to burn (basis points)
    pub auto_burn_enabled: bool,   // Auto-burn enabled
    pub burn_interval: u64,        // Burn execution interval (seconds)
    pub min_yield_threshold: u128, // Minimum yield to trigger burn
    pub project_token: Address,    // Project's native token address
    pub dead_address: Address,     // Dead address for burned tokens
    pub last_burn_amount: u128,    // Last amount burned
    pub total_burned: u128,        // Total tokens burned to date
    pub burn_count: u32,           // Number of burn operations executed
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct YieldToBurnOperation {
    pub operation_id: u64,
    pub grant_id: u64,
    pub yield_amount: u128,      // Yield generated for burning
    pub burn_amount: u128,       // Amount of tokens to buy back
    pub token_price: u128,       // Price when buy-back executed
    pub slippage_tolerance: u32, // Maximum slippage tolerance (bps)
    pub created_at: u64,
    pub executed_at: Option<u64>, // When burn was executed
    pub status: BurnOperationStatus,
    pub actual_burned: u128, // Actual amount burned after slippage
    pub gas_used: u128,      // Gas used for burn operation
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum BurnOperationStatus {
    Pending,   // Created, waiting for execution
    Executing, // Currently executing buy-back and burn
    Completed, // Successfully completed
    Failed,    // Failed due to insufficient liquidity or other error
    Cancelled, // Cancelled by admin
    Expired,   // Expired without execution
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TokenSupplyMetrics {
    pub initial_supply: u128,        // Initial token supply
    pub current_supply: u128,        // Current circulating supply
    pub total_burned: u128,          // Total tokens burned
    pub total_yield_generated: u128, // Total yield generated
    pub last_burn_timestamp: u64,    // Last burn operation timestamp
    pub burn_rate: u32,              // Current burn rate (basis points)
    pub yield_to_burn_ratio: u32,    // Percentage of yield redirected to burn
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BuyBackExecution {
    pub execution_id: u64,
    pub operation_id: u64,
    pub amount_spent: u128,    // Amount of yield spent
    pub tokens_received: u128, // Tokens bought back
    pub average_price: u128,   // Average execution price
    pub slippage: u128,        // Actual slippage incurred
    pub gas_cost: u128,        // Gas cost of execution
    pub executed_at: u64,
    pub success: bool,
}

// --- Interest Redirection Errors ---

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracterror]
#[repr(u32)]
pub enum InterestRedirectionError {
    NotInitialized = 1,
    Unauthorized = 2,
    InvalidBurnRatio = 3,
    InsufficientYield = 4,
    BurnOperationNotFound = 5,
    InvalidOperationState = 6,
    SlippageExceeded = 7,
    InsufficientLiquidity = 8,
    TokenError = 9,
    MathOverflow = 10,
    AutoBurnDisabled = 11,
    InvalidAmount = 12,
    InvalidAddress = 13,
    OperationExpired = 14,
    AlreadyExists = 15,
}

// --- Interest Redirection Data Keys ---

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum InterestRedirectionDataKey {
    Config,
    BurnOperation(u64), // operation_id -> YieldToBurnOperation
    TokenSupplyMetrics, // Token supply and burn tracking
    NextOperationId,
    PendingOperations,     // Vec<operation_id> pending execution
    BuyBackExecution(u64), // execution_id -> BuyBackExecution
    DeadAddress,           // Dead address for burns
    LastBurnTimestamp,     // Last burn operation timestamp
    YieldAccumulator,      // Accumulated yield for burning
}

// --- Interest Redirection Contract ---

#[contract]
pub struct InterestRedirectionContract;

#[contractimpl]
impl InterestRedirectionContract {
    /// Initialize interest redirection system
    pub fn initialize(
        env: Env,
        admin: Address,
        project_token: Address,
        burn_ratio: u32,
        auto_burn_enabled: bool,
    ) -> Result<(), InterestRedirectionError> {
        // Check if already initialized
        if env
            .storage()
            .instance()
            .get(&InterestRedirectionDataKey::Config)
            .is_some()
        {
            return Err(InterestRedirectionError::AlreadyExists);
        }

        // Validate parameters
        if burn_ratio < MIN_BURN_RATIO || burn_ratio > MAX_BURN_RATIO {
            return Err(InterestRedirectionError::InvalidBurnRatio);
        }

        // Create dead address
        let dead_address = Address::from_string(&env, DEAD_ADDRESS);

        let config = BurnConfig {
            admin: admin.clone(),
            burn_ratio,
            auto_burn_enabled,
            burn_interval: BURN_EXECUTION_INTERVAL,
            min_yield_threshold: MIN_YIELD_THRESHOLD,
            project_token: project_token.clone(),
            dead_address: dead_address.clone(),
            last_burn_amount: 0,
            total_burned: 0,
            burn_count: 0,
        };

        // Initialize storage
        env.storage()
            .instance()
            .set(&InterestRedirectionDataKey::Config, &config);
        env.storage()
            .instance()
            .set(&InterestRedirectionDataKey::NextOperationId, &1u64);
        env.storage().instance().set(
            &InterestRedirectionDataKey::PendingOperations,
            &Vec::new(&env),
        );
        env.storage()
            .instance()
            .set(&InterestRedirectionDataKey::DeadAddress, &dead_address);

        // Initialize token supply metrics
        let token_client = token::Client::new(&env, &project_token);
        let initial_supply = token_client.total_supply(&env.current_contract_address());

        let metrics = TokenSupplyMetrics {
            initial_supply,
            current_supply: initial_supply,
            total_burned: 0,
            total_yield_generated: 0,
            last_burn_timestamp: 0,
            burn_rate: burn_ratio,
            yield_to_burn_ratio: burn_ratio,
        };

        env.storage()
            .instance()
            .set(&InterestRedirectionDataKey::TokenSupplyMetrics, &metrics);

        env.events().publish(
            (symbol_short!("interest_redirection_initialized"),),
            (admin, project_token, burn_ratio, auto_burn_enabled),
        );

        Ok(())
    }

    /// Create burn operation from yield
    pub fn create_burn_operation(
        env: Env,
        grant_id: u64,
        yield_amount: u128,
        burn_amount: u128,
        slippage_tolerance: u32,
    ) -> Result<u64, InterestRedirectionError> {
        let config = Self::get_config(&env)?;

        // Validate amounts
        if yield_amount < config.min_yield_threshold {
            return Err(InterestRedirectionError::InsufficientYield);
        }
        if burn_amount == 0 {
            return Err(InterestRedirectionError::InvalidAmount);
        }

        let operation_id = Self::get_next_operation_id(&env);
        let now = env.ledger().timestamp();

        let operation = YieldToBurnOperation {
            operation_id,
            grant_id,
            yield_amount,
            burn_amount,
            token_price: 0, // Will be set during execution
            slippage_tolerance,
            created_at: now,
            executed_at: None,
            status: BurnOperationStatus::Pending,
            actual_burned: 0,
            gas_used: 0,
        };

        // Store operation
        env.storage().instance().set(
            &InterestRedirectionDataKey::BurnOperation(operation_id),
            &operation,
        );

        // Add to pending operations
        let mut pending = Self::get_pending_operations(&env)?;
        pending.push_back(operation_id);
        env.storage()
            .instance()
            .set(&InterestRedirectionDataKey::PendingOperations, &pending);

        // Update yield accumulator
        Self::update_yield_accumulator(&env, yield_amount)?;

        env.events().publish(
            (symbol_short!("burn_operation_created"),),
            (operation_id, grant_id, yield_amount, burn_amount),
        );

        Ok(operation_id)
    }

    /// Execute buy-back and burn operation
    pub fn execute_burn_operation(
        env: Env,
        operation_id: u64,
    ) -> Result<(), InterestRedirectionError> {
        let config = Self::get_config(&env)?;
        let mut operation = Self::get_burn_operation(&env, operation_id)?;

        // Validate operation state
        if operation.status != BurnOperationStatus::Pending {
            return Err(InterestRedirectionError::InvalidOperationState);
        }

        // Update status to executing
        operation.status = BurnOperationStatus::Executing;
        env.storage().instance().set(
            &InterestRedirectionDataKey::BurnOperation(operation_id),
            &operation,
        );

        // Get current token price (this would interface with DEX/oracle)
        let current_price = Self::get_current_token_price(&env, &config.project_token)?;
        operation.token_price = current_price;

        // Execute buy-back using yield amount
        let execution_result = Self::execute_buy_back(&env, &config, &operation, current_price)?;

        // Update operation with execution results
        operation.executed_at = Some(env.ledger().timestamp());
        operation.actual_burned = execution_result.tokens_received;
        operation.gas_used = execution_result.gas_cost;

        if execution_result.success {
            operation.status = BurnOperationStatus::Completed;

            // Update config
            let mut updated_config = config;
            updated_config.last_burn_amount = execution_result.tokens_received;
            updated_config.total_burned += execution_result.tokens_received;
            updated_config.burn_count += 1;
            env.storage()
                .instance()
                .set(&InterestRedirectionDataKey::Config, &updated_config);

            // Update token supply metrics
            Self::update_token_supply_metrics(&env, execution_result.tokens_received)?;
        } else {
            operation.status = BurnOperationStatus::Failed;
        }

        env.storage().instance().set(
            &InterestRedirectionDataKey::BurnOperation(operation_id),
            &operation,
        );

        // Remove from pending operations
        Self::remove_from_pending_operations(&env, operation_id)?;

        env.events().publish(
            (symbol_short!("burn_operation_executed"),),
            (
                operation_id,
                execution_result.tokens_received,
                execution_result.success,
            ),
        );

        Ok(())
    }

    /// Execute auto-burn for accumulated yield
    pub fn execute_auto_burn(env: Env) -> Result<Vec<u64>, InterestRedirectionError> {
        let config = Self::get_config(&env)?;

        if !config.auto_burn_enabled {
            return Err(InterestRedirectionError::AutoBurnDisabled);
        }

        let accumulated_yield = Self::get_yield_accumulator(&env)?;
        if accumulated_yield < config.min_yield_threshold {
            return Ok(Vec::new(&env)); // Nothing to burn
        }

        let now = env.ledger().timestamp();
        let last_burn = Self::get_last_burn_timestamp(&env)?;

        // Check if enough time has passed since last burn
        if now - last_burn < config.burn_interval {
            return Ok(Vec::new(&env));
        }

        // Calculate burn amount based on ratio
        let burn_amount = (accumulated_yield * config.burn_ratio as u128) / 10000u128;

        if burn_amount == 0 {
            return Ok(Vec::new(&env));
        }

        // Create and execute burn operation
        let operation_id = Self::create_burn_operation(
            &env,
            0u64, // System-generated operation
            accumulated_yield,
            burn_amount,
            500u32, // 5% default slippage tolerance
        )?;

        Self::execute_burn_operation(&env, operation_id)?;

        // Clear yield accumulator
        Self::clear_yield_accumulator(&env)?;

        let mut executed_operations = Vec::new(&env);
        executed_operations.push_back(operation_id);

        env.events().publish(
            (symbol_short!("auto_burn_executed"),),
            (executed_operations.len(), burn_amount),
        );

        Ok(executed_operations)
    }

    /// Get burn operation details
    pub fn get_burn_operation(
        env: &Env,
        operation_id: u64,
    ) -> Result<YieldToBurnOperation, InterestRedirectionError> {
        env.storage()
            .instance()
            .get(&InterestRedirectionDataKey::BurnOperation(operation_id))
            .ok_or(InterestRedirectionError::BurnOperationNotFound)
    }

    /// Get token supply metrics
    pub fn get_token_supply_metrics(
        env: &Env,
    ) -> Result<TokenSupplyMetrics, InterestRedirectionError> {
        env.storage()
            .instance()
            .get(&InterestRedirectionDataKey::TokenSupplyMetrics)
            .ok_or(InterestRedirectionError::NotInitialized)
    }

    /// Get configuration
    pub fn get_config(env: &Env) -> Result<BurnConfig, InterestRedirectionError> {
        env.storage()
            .instance()
            .get(&InterestRedirectionDataKey::Config)
            .ok_or(InterestRedirectionError::NotInitialized)
    }

    /// Update burn configuration (admin only)
    pub fn update_burn_config(
        env: Env,
        admin: Address,
        burn_ratio: Option<u32>,
        auto_burn_enabled: Option<bool>,
        burn_interval: Option<u64>,
        min_yield_threshold: Option<u128>,
    ) -> Result<(), InterestRedirectionError> {
        let mut config = Self::get_config(&env)?;

        if admin != config.admin {
            return Err(InterestRedirectionError::Unauthorized);
        }

        // Update configuration if provided
        if let Some(ratio) = burn_ratio {
            if ratio < MIN_BURN_RATIO || ratio > MAX_BURN_RATIO {
                return Err(InterestRedirectionError::InvalidBurnRatio);
            }
            config.burn_ratio = ratio;
        }

        if let Some(enabled) = auto_burn_enabled {
            config.auto_burn_enabled = enabled;
        }

        if let Some(interval) = burn_interval {
            config.burn_interval = interval;
        }

        if let Some(threshold) = min_yield_threshold {
            config.min_yield_threshold = threshold;
        }

        env.storage()
            .instance()
            .set(&InterestRedirectionDataKey::Config, &config);

        env.events().publish(
            (symbol_short!("burn_config_updated"),),
            (
                burn_ratio.unwrap_or(config.burn_ratio),
                auto_burn_enabled.unwrap_or(config.auto_burn_enabled),
            ),
        );

        Ok(())
    }

    /// Get pending burn operations
    pub fn get_pending_operations(env: &Env) -> Result<Vec<u64>, InterestRedirectionError> {
        Ok(env
            .storage()
            .instance()
            .get(&InterestRedirectionDataKey::PendingOperations)
            .unwrap_or_else(|| Vec::new(env)))
    }

    // --- Helper Functions ---

    fn get_next_operation_id(env: &Env) -> u64 {
        let id = env
            .storage()
            .instance()
            .get(&InterestRedirectionDataKey::NextOperationId)
            .unwrap_or(1u64);
        env.storage()
            .instance()
            .set(&InterestRedirectionDataKey::NextOperationId, &(id + 1));
        id
    }

    fn get_pending_operations(env: &Env) -> Result<Vec<u64>, InterestRedirectionError> {
        Ok(env
            .storage()
            .instance()
            .get(&InterestRedirectionDataKey::PendingOperations)
            .unwrap_or_else(|| Vec::new(env)))
    }

    fn get_burn_operation(
        env: &Env,
        operation_id: u64,
    ) -> Result<YieldToBurnOperation, InterestRedirectionError> {
        env.storage()
            .instance()
            .get(&InterestRedirectionDataKey::BurnOperation(operation_id))
            .ok_or(InterestRedirectionError::BurnOperationNotFound)
    }

    fn remove_from_pending_operations(
        env: &Env,
        operation_id: u64,
    ) -> Result<(), InterestRedirectionError> {
        let mut pending = Self::get_pending_operations(env)?;
        pending = pending
            .iter()
            .filter(|&&id| id != operation_id)
            .collect::<Vec<_>>(&env);
        env.storage()
            .instance()
            .set(&InterestRedirectionDataKey::PendingOperations, &pending);
        Ok(())
    }

    fn get_yield_accumulator(env: &Env) -> Result<u128, InterestRedirectionError> {
        env.storage()
            .instance()
            .get(&InterestRedirectionDataKey::YieldAccumulator)
            .unwrap_or(0u128)
    }

    fn update_yield_accumulator(env: &Env, amount: u128) -> Result<(), InterestRedirectionError> {
        let current = Self::get_yield_accumulator(env)?;
        let new_amount = current
            .checked_add(amount)
            .ok_or(InterestRedirectionError::MathOverflow)?;
        env.storage()
            .instance()
            .set(&InterestRedirectionDataKey::YieldAccumulator, &new_amount);
        Ok(())
    }

    fn clear_yield_accumulator(env: &Env) -> Result<(), InterestRedirectionError> {
        env.storage()
            .instance()
            .set(&InterestRedirectionDataKey::YieldAccumulator, &0u128);
        Ok(())
    }

    fn get_last_burn_timestamp(env: &Env) -> Result<u64, InterestRedirectionError> {
        env.storage()
            .instance()
            .get(&InterestRedirectionDataKey::LastBurnTimestamp)
            .unwrap_or(0u64)
    }

    fn get_current_token_price(
        env: &Env,
        token_address: &Address,
    ) -> Result<u128, InterestRedirectionError> {
        // This would interface with DEX or price oracle
        // For now, return a simulated price
        Ok(1000000u128) // Simulated price (1 token = 0.1 XLM)
    }

    fn execute_buy_back(
        env: &Env,
        config: &BurnConfig,
        operation: &YieldToBurnOperation,
        current_price: u128,
    ) -> Result<BuyBackExecution, InterestRedirectionError> {
        let execution_id = Self::get_next_operation_id(env);
        let now = env.ledger().timestamp();

        // Calculate expected tokens to receive
        let expected_tokens = (operation.yield_amount * 10000u128) / current_price;

        // Apply slippage tolerance
        let min_tokens =
            (expected_tokens * (10000u128 - operation.slippage_tolerance as u128)) / 10000u128;

        // Get dead address
        let dead_address = env
            .storage()
            .instance()
            .get(&InterestRedirectionDataKey::DeadAddress)
            .ok_or(InterestRedirectionError::InvalidAddress)?;

        // Execute token transfer (in reality, this would be a DEX swap)
        // For simulation, we'll transfer directly to dead address
        let token_client = token::Client::new(env, &config.project_token);

        // Check if contract has enough tokens
        let contract_balance = token_client.balance(&env.current_contract_address());
        if contract_balance < operation.burn_amount as i128 {
            return Err(InterestRedirectionError::InsufficientLiquidity);
        }

        // Transfer to dead address (simulating burn)
        token_client.transfer(
            &env.current_contract_address(),
            &dead_address,
            &operation.burn_amount,
        );

        // Calculate execution results
        let actual_slippage = expected_tokens.saturating_sub(operation.burn_amount);
        let gas_cost = 500000u128; // Simulated gas cost

        let execution = BuyBackExecution {
            execution_id,
            operation_id: operation.operation_id,
            amount_spent: operation.yield_amount,
            tokens_received: operation.burn_amount,
            average_price: current_price,
            slippage: actual_slippage,
            gas_cost,
            executed_at: now,
            success: true,
        };

        // Store execution record
        env.storage().instance().set(
            &InterestRedirectionDataKey::BuyBackExecution(execution_id),
            &execution,
        );

        Ok(execution)
    }

    fn update_token_supply_metrics(
        env: &Env,
        burned_amount: u128,
    ) -> Result<(), InterestRedirectionError> {
        let mut metrics = Self::get_token_supply_metrics(env)?;

        metrics.current_supply = metrics
            .current_supply
            .checked_sub(burned_amount)
            .ok_or(InterestRedirectionError::MathOverflow)?;
        metrics.total_burned = metrics
            .total_burned
            .checked_add(burned_amount)
            .ok_or(InterestRedirectionError::MathOverflow)?;
        metrics.last_burn_timestamp = env.ledger().timestamp();

        env.storage()
            .instance()
            .set(&InterestRedirectionDataKey::TokenSupplyMetrics, &metrics);
        Ok(())
    }
}

#[cfg(test)]
mod test_interest_redirection;
