#![allow(unexpected_cfgs)]
#![no_std]
use soroban_sdk::{contract, contracttype, contractimpl, Address, Env, token};

#[contracttype]
#[derive(Clone)]
pub struct Grant {
    pub admin: Address,
    pub grantee: Address,
    pub flow_rate: i128,
    pub balance: i128,
    pub last_claim_time: u64,
    pub is_paused: bool,
    pub token: Address,
    pub dispute_active: bool,  // frozen if true
}

#[contracttype]
pub enum DataKey {
    Grant(u64),
    Count,
    Arbiter,
}

#[contract]
pub struct GrantContract;

#[contractimpl]
impl GrantContract {
    // ────────────────────────────────────────────────
    // Added for Issue #16: Optimize Storage Bumps
    // Only extend TTL when remaining lifetime is low → saves gas on most calls
    fn ensure_sufficient_ttl(env: &Env) {
        const THRESHOLD: u32 = 1000;           // ~few hours — bump only if below this
        let max_ttl = env.storage().max_ttl();  // network maximum TTL

        // extend_ttl() is conditional: does nothing if current TTL ≥ threshold
        // It extends both instance storage and the contract WASM code entry
        env.storage().instance().extend_ttl(THRESHOLD, max_ttl);
    }
    // ────────────────────────────────────────────────

    pub fn set_arbiter(env: Env, admin: Address, arbiter: Address) {
        Self::ensure_sufficient_ttl(&env);

        admin.require_auth();

        // Only callable if arbiter not set yet (or add admin check)
        if env.storage().instance().has(&DataKey::Arbiter) {
            panic!("Arbiter already set");
        }

        env.storage().instance().set(&DataKey::Arbiter, &arbiter);
    }

    // ─── NEW: Core function for #17 ───
    pub fn set_dispute_state(env: Env, grant_id: u64, active: bool) {
        Self::ensure_sufficient_ttl(&env);

        // Only the designated arbiter can call this
        let arbiter: Address = env.storage().instance()
            .get(&DataKey::Arbiter)
            .unwrap_or_else(|| panic!("No arbiter set"));

        arbiter.require_auth();

        let mut grant: Grant = env.storage().instance()
            .get(&DataKey::Grant(grant_id))
            .unwrap_or_else(|| panic!("Grant not found"));

        grant.dispute_active = active;

        env.storage().instance().set(&DataKey::Grant(grant_id), &grant);

        // Optional: emit event for frontend
        // env.events().publish(("DisputeUpdated", grant_id), active);
    }

    pub fn create_grant(
        env: Env,
        admin: Address,
        grantee: Address,
        deposit: i128,
        flow_rate: i128,
        token: Address,
    ) -> u64 {
        Self::ensure_sufficient_ttl(&env);  // Added for #16

        admin.require_auth();

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::Count)
            .unwrap_or(0);
        count += 1;

        let client = token::Client::new(&env, &token);
        client.transfer(&admin, &env.current_contract_address(), &deposit);

        let grant = Grant {
            admin,
            grantee,
            flow_rate,
            balance: deposit,
            last_claim_time: env.ledger().timestamp(),
            is_paused: false,
            token,
            dispute_active: false,
        };

        env.storage()
            .instance()
            .set(&DataKey::Grant(count), &grant);
        env.storage().instance().set(&DataKey::Count, &count);

        count
    }

    pub fn withdraw(env: Env, grant_id: u64) {
        Self::ensure_sufficient_ttl(&env);  // Added for #16

        let mut grant: Grant = env
            .storage()
            .instance()
            .get(&DataKey::Grant(grant_id))
            .unwrap();

        grant.grantee.require_auth();

        if grant.is_paused {
            panic!("Grant PAUSED by admin");
        }

        if grant.dispute_active {
            panic!("Grant is under dispute - withdrawals blocked");
        }


        let current_time = env.ledger().timestamp();
        let seconds_passed = current_time - grant.last_claim_time;
        let amount_due = grant.flow_rate * seconds_passed as i128;

        let payout = if grant.balance >= amount_due {
            amount_due
        } else {
            grant.balance
        };

        if payout > 0 {
            let client = token::Client::new(&env, &grant.token);
            client.transfer(&env.current_contract_address(), &grant.grantee, &payout);

            grant.balance -= payout;
            grant.last_claim_time = current_time;

            env.storage()
                .instance()
                .set(&DataKey::Grant(grant_id), &grant);
        }
    }

    pub fn set_pause(env: Env, grant_id: u64, pause_state: bool) {
        Self::ensure_sufficient_ttl(&env);  // Added for #16

        let mut grant: Grant = env
            .storage()
            .instance()
            .get(&DataKey::Grant(grant_id))
            .unwrap();

        grant.admin.require_auth();

        grant.is_paused = pause_state;

        env.storage()
            .instance()
            .set(&DataKey::Grant(grant_id), &grant);
    }
}