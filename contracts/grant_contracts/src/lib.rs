#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env,
    Vec,
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
    pub debt_divert_bps: i128, // e.g., 2000 for 20%
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

const RATE_INCREASE_TIMELOCK_SECS: u64 = 48 * 60 * 60;
pub const SCALING_FACTOR: i128 = 10_000_000;
const TAX_THRESHOLD: i128 = 100_000 * 10_000_000; 
const TAX_BPS: i128 = 1;

// --- Internal Helpers ---

fn read_config(env: &Env) -> Result<ProtocolConfig, Error> {
    env.storage().instance().get(&DataKey::ProtocolConfig).ok_or(Error::ConfigNotSet)
}

fn settle_grant(env: &Env, grant: &mut Grant, now: u64) -> Result<(), Error> {
    if now < grant.last_update_ts { return Err(Error::InvalidState); }
    if grant.status != GrantStatus::Active {
        grant.last_update_ts = now;
        return Ok(());
    }

    let mut accrued_scaled: i128 = 0;
    let mut cursor = grant.last_update_ts;

    // 1. Process Timelock logic for Rate Increases
    if grant.pending_rate > grant.flow_rate && grant.effective_timestamp != 0 {
        let activation_ts = grant.effective_timestamp;
        if cursor < activation_ts {
            let pre_end = if now < activation_ts { now } else { activation_ts };
            accrued_scaled = (pre_end - cursor) as i128 * grant.flow_rate;
            cursor = pre_end;
        }
        if now >= activation_ts {
            grant.flow_rate = grant.pending_rate;
            grant.rate_updated_at = activation_ts;
            grant.pending_rate = 0;
            grant.effective_timestamp = 0;
        }
    }

    // 2. Accrue remaining time
    if cursor < now {
        accrued_scaled += (now - cursor) as i128 * grant.flow_rate;
    }

    if accrued_scaled <= 0 {
        grant.last_update_ts = now;
        return Ok(());
    }

    // 3. Apply Warmup Multiplier and Scale back to Token Units
    let multiplier = calculate_warmup_multiplier(grant, now);
    let mut delta = (accrued_scaled * multiplier) / (SCALING_FACTOR * 10000);

    // Cap at total grant amount
    let remaining = grant.total_amount - (grant.withdrawn + grant.claimable);
    if delta > remaining { delta = remaining; }
    if delta <= 0 {
        grant.last_update_ts = now;
        return Ok(());
    }

    let config = read_config(env)?;
    let mut net_delta = delta;

    // 4. Sustainability Tax Logic
    if grant.total_amount >= TAX_THRESHOLD {
        let tax = (delta * TAX_BPS) / 10000;
        if tax > 0 {
            env.invoke_contract::<()>(
                &config.treasury_address,
                &symbol_short!("deposit"),
                soroban_sdk::vec![env, tax],
            );
            net_delta -= tax;
        }
    }

    // 5. Debt Service Logic
    if grant.sorosusu_debt_service {
        let is_default: bool = env.invoke_contract(&config.sorosusu_address, &symbol_short!("is_deflt"), soroban_sdk::vec![env, grant.recipient.clone()]);
        if is_default {
            let debt_payment = (delta * config.debt_divert_bps) / 10000;
            if debt_payment > 0 {
                env.invoke_contract::<()>(
                    &config.sorosusu_address,
                    &symbol_short!("repay"),
                    soroban_sdk::vec![env, grant.recipient.clone(), debt_payment],
                );
                net_delta -= debt_payment;
            }
        }
    }

    grant.claimable += net_delta;
    grant.total_volume_serviced += delta;
    
    if (grant.withdrawn + grant.claimable) >= grant.total_amount {
        grant.status = GrantStatus::Completed;
    }

    grant.last_update_ts = now;
    Ok(())
}

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

    pub fn withdraw(env: Env, grant_id: u64, amount: i128) -> Result<(), Error> {
        let mut grant: Grant = env.storage().instance().get(&DataKey::Grant(grant_id)).ok_or(Error::GrantNotFound)?;
        
        // Authorization: Both partners must sign if it's a joint grant
        grant.recipient.require_auth();
        if let Some(info) = &grant.joint_info {
            info.partner.require_auth();
        }

        settle_grant(&env, &mut grant, env.ledger().timestamp())?;

        if amount > grant.claimable || amount <= 0 { return Err(Error::InvalidAmount); }

        grant.claimable -= amount;
        grant.withdrawn += amount;
        grant.last_claim_time = env.ledger().timestamp();

        let token_client = token::Client::new(&env, &env.storage().instance().get(&DataKey::GrantToken).unwrap());
        token_client.transfer(&env.current_contract_address(), &grant.recipient, &amount);

        // SBT Minting on Completion
        if grant.status == GrantStatus::Completed {
            if let Ok(config) = read_config(&env) {
                let _: () = env.invoke_contract(&config.sbt_minter_address, &symbol_short!("mint_sbt"), soroban_sdk::vec![env, grant_id, grant.recipient.clone()]);
            }
        }

        env.storage().instance().set(&DataKey::Grant(grant_id), &grant);
        Ok(())
    }
}

fn calculate_warmup_multiplier(grant: &Grant, now: u64) -> i128 {
    if grant.warmup_duration == 0 || now >= grant.start_time + grant.warmup_duration { return 10000; }
    if now <= grant.start_time { return 2500; }
    let progress = ((now - grant.start_time) as i128 * 10000) / (grant.warmup_duration as i128);
    2500 + (7500 * progress / 10000)
}