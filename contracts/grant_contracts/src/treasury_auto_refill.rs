#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, token, symbol_short};

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct AutoRefillConfig {
    pub treasury: Address,
    pub secondary_asset: Address,
    pub grant_asset: Address,
    pub dex_router: Address,
    pub thirty_day_burn_rate: i128,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub enum RefillDataKey {
    Config,
    LastRefill,
}

#[contract]
pub struct TreasuryAutoRefill;

#[contractimpl]
impl TreasuryAutoRefill {
    pub fn configure(
        env: Env, admin: Address, treasury: Address, secondary_asset: Address,
        grant_asset: Address, dex_router: Address, thirty_day_burn_rate: i128,
    ) {
        admin.require_auth();
        let config = AutoRefillConfig { treasury, secondary_asset, grant_asset, dex_router, thirty_day_burn_rate, enabled: true };
        env.storage().instance().set(&RefillDataKey::Config, &config);
    }

    pub fn check_and_trigger_refill(env: Env) -> bool {
        if let Some(config) = env.storage().instance().get::<_, AutoRefillConfig>(&RefillDataKey::Config) {
            if !config.enabled { return false; }
            
            let grant_token = token::Client::new(&env, &config.grant_asset);
            let treasury_balance = grant_token.balance(&config.treasury);
            
            if treasury_balance < config.thirty_day_burn_rate {
                let required_amount = config.thirty_day_burn_rate - treasury_balance;
                env.events().publish((symbol_short!("auto_refill"), config.treasury.clone()), 
                    (config.secondary_asset, config.grant_asset, required_amount));
                env.storage().instance().set(&RefillDataKey::LastRefill, &env.ledger().timestamp());
                return true;
            }
        }
        false
    }
}