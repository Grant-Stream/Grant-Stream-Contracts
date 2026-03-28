use soroban_sdk::{contractimpl, Address, Env, Symbol, Vec, Map};
use crate::grant_contract::{GrantContract, GrantId, Collateral, GrantError};

#[derive(Clone)]
pub struct LpPosition {
    pub lp_token: Address,        // Address of the LP share token (SAC or custom)
    pub amount_staked: u128,
    pub pool_address: Option<Address>, // Optional: reference to the actual liquidity pool
    pub accrued_fees: u128,       // Track fees/rewards earned while staked
}

pub trait LpStakingTrait {
    // Stake LP tokens as collateral for a grant
    fn stake_lp_as_collateral(env: Env, grant_id: GrantId, lp_token: Address, amount: u128, pool: Option<Address>);

    // Claim loyalty bonus (LP fees) when milestones are successfully met
    fn claim_loyalty_bonus(env: Env, grant_id: GrantId) -> u128;

    // Slash LP collateral (called by DAO or on default)
    fn slash_lp_collateral(env: Env, grant_id: GrantId, percentage: u32); // e.g. 100 for full slash

    // View LP positions and accrued rewards
    fn get_lp_positions(env: Env, grant_id: GrantId) -> Vec<LpPosition>;
}

#[contractimpl]
impl LpStakingTrait for GrantContract {
    fn stake_lp_as_collateral(env: Env, grant_id: GrantId, lp_token: Address, amount: u128, pool: Option<Address>) {
        let mut grant = Self::get_grant(&env, grant_id);
        // Authorization & validation
        grant.admin.require_auth(); // or grantee depending on flow

        // Transfer LP tokens from grantee to this contract (escrow)
        let client = soroban_sdk::token::Client::new(&env, &lp_token);
        client.transfer(&env.current_contract_address(), &grant.grantee, &(amount as i128)); // adjust types as needed

        let position = LpPosition {
            lp_token: lp_token.clone(),
            amount_staked: amount,
            pool_address: pool,
            accrued_fees: 0,
        };

        grant.collateral.lp_positions.push(position); // Extend existing collateral struct
        Self::save_grant(&env, grant_id, grant);
        env.events().publish((Symbol::new(&env, "lp_staked"), grant_id), amount);
    }

    fn claim_loyalty_bonus(env: Env, grant_id: GrantId) -> u128 {
        let grant = Self::get_grant(&env, grant_id);
        if !grant.is_completed_successfully() {
            panic_with_error!(&env, GrantError::GrantNotEligible);
        }

        let mut total_bonus = 0u128;
        for pos in grant.collateral.lp_positions.iter() {
            // In real implementation: query the LP pool or use a reward distributor
            // For MVP: simulate or use a simple fee accrual
            let bonus = pos.accrued_fees; // or calculate share of pool fees
            if bonus > 0 {
                let token_client = soroban_sdk::token::Client::new(&env, &pos.lp_token);
                token_client.transfer(&env.current_contract_address(), &grant.grantee, &(bonus as i128));
                total_bonus += bonus;
            }
        }

        env.events().publish((Symbol::new(&env, "loyalty_bonus_claimed"), grant_id), total_bonus);
        total_bonus
    }

    fn slash_lp_collateral(env: Env, grant_id: GrantId, percentage: u32) {
        // Only DAO / admin can slash
        // ... authorization check ...

        let mut grant = Self::get_grant(&env, grant_id);
        for pos in grant.collateral.lp_positions.iter_mut() {
            let slash_amount = (pos.amount_staked * percentage as u128) / 100;
            if slash_amount > 0 {
                let token_client = soroban_sdk::token::Client::new(&env, &pos.lp_token);
                token_client.transfer(&env.current_contract_address(), &grant.dao_treasury, &(slash_amount as i128));
                pos.amount_staked -= slash_amount;
            }
        }
        Self::save_grant(&env, grant_id, grant);
    }

    fn get_lp_positions(env: Env, grant_id: GrantId) -> Vec<LpPosition> {
        let grant = Self::get_grant(&env, grant_id);
        grant.collateral.lp_positions.clone()
    }
}