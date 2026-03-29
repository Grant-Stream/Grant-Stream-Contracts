#![no_std]
use soroban_sdk::{contract, contracttype, contractimpl, Address, Env, token, symbol_short, Symbol};

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
    pub dispute_active: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct SubStream {
    pub creator: Address,
    pub subscriber: Address,
    pub flow_rate: i128,
    pub balance: i128,
    pub last_claim_time: u64,
    pub is_active: bool,
}

#[contracttype]
pub enum DataKey {
    Grant(u64),
    SubStream(u64),
    GrantCount,
    SubStreamCount,
    Arbiter,
}

#[contract]
pub struct GrantContract;

#[contractimpl]
impl GrantContract {
    fn ensure_sufficient_ttl(env: &Env) {
        const THRESHOLD: u32 = 1000;
        let max_ttl = env.storage().max_ttl();
        env.storage().instance().extend_ttl(THRESHOLD, max_ttl);
    }
/// Calculate the real-world purchasing power of a grant amount based on a given inflation rate.
    /// inflation_rate_bps is expected as a basis point (e.g., 500 = 5%)
    pub fn get_grantee_spending_power(env: Env, amount: i128, inflation_rate_bps: i128) -> i128 {
        if amount <= 0 {
            return 0;
        }

        // Basis points scaling factor (10,000 = 100.00%)
        let bps_divisor = 10_000;
        
        // Formula: Amount / (1 + Rate)
        // Scaled for BPS logic: (Amount * 10,000) / (10,000 + Rate_BPS)
        let adjusted_power = (amount * bps_divisor) / (bps_divisor + inflation_rate_bps);

        adjusted_power
    }
    // ─── Bridge: Use SubStream revenue as collateral for Grant ───
    pub fn use_substream_as_collateral(env: Env, grant_id: u64, substream_id: u64) -> bool {
        Self::ensure_sufficient_ttl(&env);

        let mut grant: Grant = env.storage().instance()
            .get(&DataKey::Grant(grant_id))
            .unwrap_or_else(|| panic!("Grant not found"));

        let substream: SubStream = env.storage().instance()
            .get(&DataKey::SubStream(substream_id))
            .unwrap_or_else(|| panic!("SubStream not found"));

        // Authorization: Only admin or grantee can bridge
        if env.invoker() != grant.admin && env.invoker() != grant.grantee {
            panic!("Unauthorized: only admin or grantee can bridge SubStream");
        }

        // Check SubStream is active and has balance
        if !substream.is_active || substream.balance <= 0 {
            panic!("Insufficient or inactive SubStream balance");
        }

        // Use SubStream balance as collateral
        grant.balance += substream.balance;

        env.storage().instance().set(&DataKey::Grant(grant_id), &grant);

        // Optional: emit event
        // env.events().publish(("SubStreamBridged", grant_id, substream_id), substream.balance);

        true
    }

    pub fn set_arbiter(env: Env, admin: Address, arbiter: Address) {
        Self::ensure_sufficient_ttl(&env);
        admin.require_auth();

        if env.storage().instance().has(&DataKey::Arbiter) {
            panic!("Arbiter already set");
        }

        env.storage().instance().set(&DataKey::Arbiter, &arbiter);
    }

    pub fn create_grant(
        env: Env,
        admin: Address,
        grantee: Address,
        deposit: i128,
        flow_rate: i128,
        token: Address,
    ) -> u64 {
        Self::ensure_sufficient_ttl(&env);
        admin.require_auth();

        let mut count: u64 = env.storage().instance().get(&DataKey::GrantCount).unwrap_or(0);
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

        env.storage().instance().set(&DataKey::Grant(count), &grant);
        env.storage().instance().set(&DataKey::GrantCount, &count);

        count
    }

    pub fn withdraw(env: Env, grant_id: u64) {
        Self::ensure_sufficient_ttl(&env);

        let mut grant: Grant = env.storage().instance()
            .get(&DataKey::Grant(grant_id))
            .unwrap_or_else(|| panic!("Grant not found"));

        grant.grantee.require_auth();

        if grant.is_paused || grant.dispute_active {
            panic!("Grant is paused or under dispute");
        }

        let current_time = env.ledger().timestamp();
        let seconds_passed = current_time - grant.last_claim_time;
        let amount_due = grant.flow_rate * seconds_passed as i128;

        let payout = if grant.balance >= amount_due { amount_due } else { grant.balance };

        if payout > 0 {
            let client = token::Client::new(&env, &grant.token);
            client.transfer(&env.current_contract_address(), &grant.grantee, &payout);

            grant.balance -= payout;
            grant.last_claim_time = current_time;

            env.storage().instance().set(&DataKey::Grant(grant_id), &grant);
        }
    }

    pub fn set_pause(env: Env, grant_id: u64, pause_state: bool) {
        Self::ensure_sufficient_ttl(&env);

        let mut grant: Grant = env.storage().instance()
            .get(&DataKey::Grant(grant_id))
            .unwrap_or_else(|| panic!("Grant not found"));

        grant.admin.require_auth();
        grant.is_paused = pause_state;

        env.storage().instance().set(&DataKey::Grant(grant_id), &grant);
    }
}

// ────────────────────────────────────────────────
// SubStream Contract
// ────────────────────────────────────────────────

#[contract]
pub struct SubStreamContract;

#[contractimpl]
impl SubStreamContract {
    pub fn create_substream(
        env: Env,
        creator: Address,
        subscriber: Address,
        flow_rate: i128,
        token: Address,
    ) -> u64 {
        Self::ensure_sufficient_ttl(&env);

        creator.require_auth();

        let mut count: u64 = env.storage().instance().get(&DataKey::SubStreamCount).unwrap_or(0);
        count += 1;

        let substream = SubStream {
            creator,
            subscriber,
            flow_rate,
            balance: 0,
            last_claim_time: env.ledger().timestamp(),
            is_active: true,
        };

        env.storage().instance().set(&DataKey::SubStream(count), &substream);
        env.storage().instance().set(&DataKey::SubStreamCount, &count);

        count
    }
}

// Shared helper
fn ensure_sufficient_ttl(env: &Env) {
    const THRESHOLD: u32 = 1000;
    let max_ttl = env.storage().max_ttl();
    env.storage().instance().extend_ttl(THRESHOLD, max_ttl);
}