#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
};

#[contract]
pub struct GrantContract;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum GrantStatus {
    Active,
    Completed,
    Cancelled,
}

#[derive(Clone)]
#[contracttype]
pub struct JointGrantInfo {
    pub partner: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct Grant {
    pub recipient: Address,
    pub total_amount: i128,
    pub withdrawn: i128,
    pub claimable: i128,
    pub flow_rate: i128,
    pub last_update_ts: u64,
    pub rate_updated_at: u64,
    pub last_claim_time: u64,
    pub pending_rate: i128,
    pub effective_timestamp: u64,
    pub status: GrantStatus,
    pub joint_info: Option<JointGrantInfo>,
    pub sorosusu_debt_service: bool,
    pub total_volume_serviced: i128,
    pub start_time: u64,
    pub warmup_duration: u64,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    GrantToken,
    Treasury,
    Oracle,
    GrantIds,
    Grant(u64),
    RecipientGrants(Address),
    ProtocolConfig,
}

#[derive(Clone)]
#[contracttype]
pub struct ProtocolConfig {
    pub sorosusu_address: Address,
    pub treasury_address: Address,
    pub sbt_minter_address: Address,
    pub debt_divert_bps: i128,
}

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
    GrantNotFound = 4,
    GrantAlreadyExists = 5,
    InvalidRate = 6,
    InvalidAmount = 7,
    InvalidState = 8,
    MathOverflow = 9,
    ConfigNotSet = 10,
}

pub const SCALING_FACTOR: i128 = 10_000_000;

#[contractimpl]
impl GrantContract {
    pub fn initialize(env: Env, admin: Address, grant_token: Address, treasury: Address, oracle: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) { return Err(Error::AlreadyInitialized); }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::GrantToken, &grant_token);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::Oracle, &oracle);
        env.storage().instance().set(&DataKey::GrantIds, &Vec::<u64>::new(&env));
        Ok(())
    }

    pub fn set_protocol_config(env: Env, sorosusu: Address, treasury: Address, sbt_minter: Address, debt_divert_bps: i128) -> Result<(), Error> {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotInitialized)?;
        admin.require_auth();
        let config = ProtocolConfig {
            sorosusu_address: sorosusu,
            treasury_address: treasury,
            sbt_minter_address: sbt_minter,
            debt_divert_bps,
        };
        env.storage().instance().set(&DataKey::ProtocolConfig, &config);
        Ok(())
    }

    pub fn create_grant(env: Env, grant_id: u64, recipient: Address, total_amount: i128, flow_rate: i128, warmup_duration: u64, partner: Option<Address>, auto_debt_service: bool) -> Result<(), Error> {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotInitialized)?;
        admin.require_auth();
        let now = env.ledger().timestamp();
        let grant = Grant {
            recipient: recipient.clone(),
            total_amount,
            withdrawn: 0,
            claimable: 0,
            flow_rate,
            last_update_ts: now,
            rate_updated_at: now,
            last_claim_time: now,
            pending_rate: 0,
            effective_timestamp: 0,
            status: GrantStatus::Active,
            joint_info: partner.map(|p| JointGrantInfo { partner: p }),
            sorosusu_debt_service: auto_debt_service,
            total_volume_serviced: 0,
            start_time: now,
            warmup_duration,
        };
        env.storage().instance().set(&DataKey::Grant(grant_id), &grant);
        Ok(())
    }

    pub fn withdraw(env: Env, grant_id: u64, amount: i128) -> Result<(), Error> {
        let mut grant: Grant = env.storage().instance().get(&DataKey::Grant(grant_id)).ok_or(Error::GrantNotFound)?;
        grant.recipient.require_auth();
        if let Some(info) = &grant.joint_info { info.partner.require_auth(); }
        
        let now = env.ledger().timestamp();
        let elapsed = (now - grant.last_update_ts) as i128;
        let mut delta = (elapsed * grant.flow_rate) / SCALING_FACTOR;

        if grant.warmup_duration > 0 {
            let end = grant.start_time + grant.warmup_duration;
            let multiplier = if now >= end { 10000 } else if now <= grant.start_time { 2500 } else {
                2500 + (7500 * ((now - grant.start_time) as i128 * 10000 / grant.warmup_duration as i128) / 10000)
            };
            delta = (delta * multiplier) / 10000;
        }

        grant.claimable += delta;
        if amount > grant.claimable || amount <= 0 { return Err(Error::InvalidAmount); }
        
        grant.claimable -= amount;
        grant.withdrawn += amount;
        grant.last_update_ts = now;
        grant.last_claim_time = now;

        let token_addr: Address = env.storage().instance().get(&DataKey::GrantToken).unwrap();
        token::Client::new(&env, &token_addr).transfer(&env.current_contract_address(), &grant.recipient, &amount);
        env.storage().instance().set(&DataKey::Grant(grant_id), &grant);
        Ok(())
    }

    pub fn get_grant(env: Env, grant_id: u64) -> Result<Grant, Error> {
        env.storage().instance().get(&DataKey::Grant(grant_id)).ok_or(Error::GrantNotFound)
    }

    pub fn claimable(env: Env, grant_id: u64) -> i128 {
        if let Ok(grant) = env.storage().instance().get::<_, Grant>(&DataKey::Grant(grant_id)) {
            let elapsed = (env.ledger().timestamp() - grant.last_update_ts) as i128;
            grant.claimable + ((elapsed * grant.flow_rate) / SCALING_FACTOR)
        } else { 0 }
    }

    pub fn slash_inactive_grant(env: Env, grant_id: u64) -> Result<(), Error> {
        let mut grant: Grant = env.storage().instance().get(&DataKey::Grant(grant_id)).ok_or(Error::GrantNotFound)?;
        grant.status = GrantStatus::Cancelled;
        env.storage().instance().set(&DataKey::Grant(grant_id), &grant);
        Ok(())
    }
}