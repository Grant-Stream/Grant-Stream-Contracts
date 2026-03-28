use soroban_sdk::{contractimpl, Address, Env, Symbol, Map, Vec};
use crate::grant_contract::{GrantContract, GrantId, Grant, GrantError};

#[derive(Clone)]
pub struct ReserveAllocation {
    pub asset: Address,           // e.g. BENJI token address
    pub allocated_amount: u128,   // Amount moved to yield-bearing asset
    pub principal_reserved: u128, // Portion of this that must remain as principal
    pub yield_accrued: u128,
    pub allocation_timestamp: u64,
}

pub trait YieldReserveTrait {
    // DAO allocates a percentage of unstreamed treasury to yield
    fn allocate_to_yield(env: Env, asset: Address, percentage: u32); // e.g. 40 = 40%

    // Withdraw yield (principal stays protected)
    fn harvest_yield(env: Env, asset: Address) -> u128;

    // Withdraw principal only in emergency (with DAO approval)
    fn emergency_withdraw_principal(env: Env, asset: Address, amount: u128);

    // Get current liquid vs yield-bearing breakdown
    fn get_reserve_status(env: Env) -> (u128, u128, u128); // (liquid, in_yield, accrued_yield)

    // Check if enough liquidity for next 30 days of streams
    fn check_liquidity_safety(env: Env) -> bool;
}

#[contractimpl]
impl YieldReserveTrait for GrantContract {
    fn allocate_to_yield(env: Env, asset: Address, percentage: u32) {
        // Only DAO admin
        // ... authorization check (use existing DAO logic) ...

        if percentage > 70 { // Configurable max, e.g. 70%
            panic_with_error!(&env, GrantError::InvalidAllocation);
        }

        let total_unstreamed = Self::calculate_total_unstreamed(&env);
        let amount_to_allocate = (total_unstreamed * percentage as u128) / 100;

        let next_30_days = Self::calculate_next_30_days_obligations(&env);
        let minimum_liquid = next_30_days * 120 / 100; // 20% buffer

        if amount_to_allocate + minimum_liquid > total_unstreamed {
            panic_with_error!(&env, GrantError::InsufficientLiquidity);
        }

        // Transfer to yield asset (assuming it's a standard token client)
        let token = soroban_sdk::token::Client::new(&env, &asset);
        token.transfer(&env.current_contract_address(), &env.current_contract_address(), &(amount_to_allocate as i128)); 
        // In production: call deposit function on lending protocol if needed

        let mut allocation = ReserveAllocation {
            asset: asset.clone(),
            allocated_amount: amount_to_allocate,
            principal_reserved: amount_to_allocate, // initially all is principal
            yield_accrued: 0,
            allocation_timestamp: env.ledger().timestamp(),
        };

        Self::save_allocation(&env, allocation);
        env.events().publish((Symbol::new(&env, "yield_allocated"),), (asset, amount_to_allocate));
    }

    fn harvest_yield(env: Env, asset: Address) -> u128 {
        // ... auth check ...

        let mut allocation = Self::get_allocation(&env, &asset);
        let current_balance = Self::get_current_balance_of(&env, &asset); // query token balance

        let yield_earned = current_balance.saturating_sub(allocation.allocated_amount);

        if yield_earned > 0 {
            allocation.yield_accrued += yield_earned;
            // Transfer yield to DAO treasury or designated receiver
            let token = soroban_sdk::token::Client::new(&env, &asset);
            token.transfer(&env.current_contract_address(), &Self::get_dao_treasury(&env), &(yield_earned as i128));
        }

        Self::save_allocation(&env, allocation);
        env.events().publish((Symbol::new(&env, "yield_harvested"),), yield_earned);
        yield_earned
    }

    fn check_liquidity_safety(env: Env) -> bool {
        let next_30_days = Self::calculate_next_30_days_obligations(&env);
        let liquid_balance = Self::get_liquid_balance(&env); // across supported assets

        liquid_balance >= next_30_days * 110 / 100 // 10% safety buffer
    }

    fn get_reserve_status(env: Env) -> (u128, u128, u128) {
        // Returns (total_liquid, total_in_yield, total_accrued_yield)
        // Implementation aggregates from allocations map
        (0, 0, 0) // placeholder - expand with storage map
    }
}