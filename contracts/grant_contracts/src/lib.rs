#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol, Vec, Map, U256};

#[contract]
pub struct GrantContract;

const TOTAL_AMOUNT: Symbol = symbol_short!("TOTAL");
const START_TIME: Symbol = symbol_short!("START");
const END_TIME: Symbol = symbol_short!("END");
const RECIPIENT: Symbol = symbol_short!("RECIPIENT");
const CLAIMED: Symbol = symbol_short!("CLAIMED");

#[contractimpl]
impl GrantContract {
    pub fn initialize_grant(
        env: Env,
        recipient: Address,
        total_amount: U256,
        duration_seconds: u64,
    ) -> u64 {
        let start_time = env.ledger().timestamp();
        let end_time = start_time + duration_seconds;
        
        env.storage().instance().set(&TOTAL_AMOUNT, &total_amount);
        env.storage().instance().set(&START_TIME, &start_time);
        env.storage().instance().set(&END_TIME, &end_time);
        env.storage().instance().set(&RECIPIENT, &recipient);
        env.storage().instance().set(&CLAIMED, &U256::from_u64(0));
        
        end_time
    }
    
    pub fn claimable_balance(env: Env) -> U256 {
        let current_time = env.ledger().timestamp();
        let start_time = env.storage().instance().get(&START_TIME).unwrap_or(0);
        let end_time = env.storage().instance().get(&END_TIME).unwrap_or(0);
        let total_amount = env.storage().instance().get(&TOTAL_AMOUNT).unwrap_or(U256::from_u64(0));
        let claimed = env.storage().instance().get(&CLAIMED).unwrap_or(U256::from_u64(0));
        
        if current_time <= start_time {
            return U256::from_u64(0);
        }
        
        let elapsed = if current_time >= end_time {
            end_time - start_time
        } else {
            current_time - start_time
        };
        
        let total_duration = end_time - start_time;
        let vested = if total_duration > 0 {
            total_amount * U256::from_u64(elapsed) / U256::from_u64(total_duration)
        } else {
            U256::from_u64(0)
        };
        
        if vested > claimed {
            vested - claimed
        } else {
            U256::from_u64(0)
        }
    }
    
    pub fn claim(env: Env, recipient: Address) -> U256 {
        recipient.require_auth();
        
        let stored_recipient = env.storage().instance().get(&RECIPIENT).unwrap();
        assert_eq!(recipient, stored_recipient, "Unauthorized recipient");
        
        let claimable = Self::claimable_balance(env.clone());
        assert!(claimable > U256::from_u64(0), "No tokens to claim");
        
        let claimed = env.storage().instance().get(&CLAIMED).unwrap_or(U256::from_u64(0));
        let new_claimed = claimed + claimable;
        env.storage().instance().set(&CLAIMED, &new_claimed);
        
        claimable
    }
    
    pub fn get_grant_info(env: Env) -> (U256, u64, u64, U256) {
        let total_amount = env.storage().instance().get(&TOTAL_AMOUNT).unwrap_or(U256::from_u64(0));
        let start_time = env.storage().instance().get(&START_TIME).unwrap_or(0);
        let end_time = env.storage().instance().get(&END_TIME).unwrap_or(0);
        let claimed = env.storage().instance().get(&CLAIMED).unwrap_or(U256::from_u64(0));
        
        (total_amount, start_time, end_time, claimed)
    }
}

mod test;
