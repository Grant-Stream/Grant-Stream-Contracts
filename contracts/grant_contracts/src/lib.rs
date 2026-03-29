#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, vec, Address, Env,
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
    pub joint_info: Option<JointGrantInfo>,    // Issue #223
    pub sorosusu_debt_service: bool,           // Issue #213
    pub total_volume_serviced: i128,           // Track for Issue #233
    pub start_time: u64,                       // from certificates branch
    pub warmup_duration: u64,                  // from certificates branch
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
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
    JointAuthRequired = 11,
    RescueWouldViolateAllocated = 12,
    GranteeMismatch = 13,
    GrantNotInactive = 14,
}

const INACTIVITY_THRESHOLD_SECS: u64 = 90 * 24 * 60 * 60; // 90 days
const RATE_INCREASE_TIMELOCK_SECS: u64 = 48 * 60 * 60;   // 48 hours
pub const SCALING_FACTOR: i128 = 10_000_000;           // 1e7
const TAX_THRESHOLD: i128 = 100_000_0000000;           // $100,000
const TAX_BPS: i128 = 1;                               // 0.01%

fn read_admin(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotInitialized)
}

fn require_admin_auth(env: &Env) -> Result<(), Error> {
    let admin = read_admin(env)?;
    admin.require_auth();
    Ok(())
}

fn read_oracle(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Oracle).ok_or(Error::NotInitialized)
}

fn require_oracle_auth(env: &Env) -> Result<(), Error> {
    let oracle = read_oracle(env)?;
    oracle.require_auth();
    Ok(())
}

fn read_grant(env: &Env, grant_id: u64) -> Result<Grant, Error> {
    env.storage().instance().get(&DataKey::Grant(grant_id)).ok_or(Error::GrantNotFound)
}

fn write_grant(env: &Env, grant_id: u64, grant: &Grant) {
    env.storage().instance().set(&DataKey::Grant(grant_id), grant);
}

fn read_config(env: &Env) -> Result<ProtocolConfig, Error> {
    env.storage().instance().get(&DataKey::ProtocolConfig).ok_or(Error::ConfigNotSet)
}

fn is_in_default(env: &Env, sorosusu: &Address, user: &Address) -> bool {
    env.invoke_contract::<bool>(sorosusu, &symbol_short!("is_deflt"), soroban_sdk::vec![env, user.clone()])
}

fn mint_sbt(env: &Env, config: &ProtocolConfig, grant_id: u64, recipient: &Address) {
    let _: () = env.invoke_contract(
        &config.sbt_minter_address,
        &symbol_short!("mint_sbt"),
        soroban_sdk::vec![env, grant_id, recipient.clone()],
    );
}

fn read_grant_token(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::GrantToken).ok_or(Error::NotInitialized)
}

fn read_treasury(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Treasury).ok_or(Error::NotInitialized)
}

fn read_grant_ids(env: &Env) -> Vec<u64> {
    env.storage().instance().get(&DataKey::GrantIds).unwrap_or_else(|| Vec::new(env))
}

fn calculate_warmup_multiplier(grant: &Grant, now: u64) -> i128 {
    if grant.warmup_duration == 0 {
        return 10000;
    }
    let warmup_end = grant.start_time + grant.warmup_duration;
    if now >= warmup_end {
        return 10000;
    }
    if now <= grant.start_time {
        return 2500;
    }
    let elapsed_warmup = now - grant.start_time;
    let progress = (elapsed_warmup as i128 * 10000) / (grant.warmup_duration as i128);
    2500 + (7500 * progress / 10000)
}

fn settle_grant(env: &Env, grant: &mut Grant, now: u64) -> Result<(), Error> {
    if now < grant.last_update_ts {
        return Err(Error::InvalidState);
    }
    if grant.status != GrantStatus::Active {
        grant.last_update_ts = now;
        return Ok(());
    }

    let start = grant.last_update_ts;
    let mut accrued_scaled: i128 = 0;
    let mut cursor = start;

    // Handle pending rate increase timelock
    if grant.pending_rate > grant.flow_rate && grant.effective_timestamp != 0 {
        let activation_ts = grant.effective_timestamp;
        if cursor < activation_ts {
            let pre_end = if now < activation_ts { now } else { activation_ts };
            let pre_elapsed = pre_end - cursor;
            accrued_scaled = accrued_scaled.checked_add(
                grant.flow_rate.checked_mul(i128::from(pre_elapsed)).ok_or(Error::MathOverflow)?
            ).ok_or(Error::MathOverflow)?;
            cursor = pre_end;
        }
        if now >= activation_ts {
            grant.flow_rate = grant.pending_rate;
            grant.rate_updated_at = activation_ts;
            grant.pending_rate = 0;
            grant.effective_timestamp = 0;
        }
    }

    if cursor < now {
        let post_elapsed = now - cursor;
        accrued_scaled = accrued_scaled.checked_add(
            grant.flow_rate.checked_mul(i128::from(post_elapsed)).ok_or(Error::MathOverflow)?
        ).ok_or(Error::MathOverflow)?;
    }

    if accrued_scaled == 0 {
        grant.last_update_ts = now;
        return Ok(());
    }

    // Convert from scaled values to token units
    let mut delta = accrued_scaled.checked_div(SCALING_FACTOR).ok_or(Error::MathOverflow)?;

    // Apply warmup multiplier
    let multiplier = calculate_warmup_multiplier(grant, now);
    delta = delta.checked_mul(multiplier).ok_or(Error::MathOverflow)?
        .checked_div(10000).ok_or(Error::MathOverflow)?;

    let accounted = grant.withdrawn.checked_add(grant.claimable).ok_or(Error::MathOverflow)?;
    let remaining = grant.total_amount.checked_sub(accounted).ok_or(Error::MathOverflow)?;

    if delta > remaining {
        delta = remaining;
    }

    if delta == 0 {
        grant.last_update_ts = now;
        return Ok(());
    }

    let config = read_config(env)?;
    let mut net_delta = delta;

    // Issue #233: Sustainability Tax
    if grant.total_amount >= TAX_THRESHOLD {
        let tax = delta.checked_mul(TAX_BPS).unwrap().checked_div(10000).unwrap();
        if tax > 0 {
            env.invoke_contract::<()>(
                &config.treasury_address,
                &symbol_short!("deposit"),
                soroban_sdk::vec![env, tax],
            );
            net_delta = net_delta.checked_sub(tax).ok_or(Error::MathOverflow)?;
        }
    }

    // Issue #213: Debt Repayment Drip
    if grant.sorosusu_debt_service {
        if is_in_default(env, &config.sorosusu_address, &grant.recipient) {
            let debt_service = delta.checked_mul(config.debt_divert_bps).unwrap().checked_div(10000).unwrap();
            if debt_service > 0 {
                env.invoke_contract::<()>(
                    &config.sorosusu_address,
                    &symbol_short!("repay"),
                    soroban_sdk::vec![env, grant.recipient.clone(), debt_service],
                );
                net_delta = net_delta.checked_sub(debt_service).ok_or(Error::MathOverflow)?;
            }
        }
    }

    grant.claimable = grant.claimable.checked_add(net_delta).ok_or(Error::MathOverflow)?;
    grant.total_volume_serviced = grant.total_volume_serviced.checked_add(delta).ok_or(Error::MathOverflow)?;
    
    let new_accounted = grant.withdrawn.checked_add(grant.claimable).ok_or(Error::MathOverflow)?;
    if new_accounted >= grant.total_amount {
        grant.status = GrantStatus::Completed;
    }

    grant.last_update_ts = now;
    Ok(())
}

fn preview_grant_at_now(env: &Env, grant: &Grant) -> Result<Grant, Error> {
    let mut preview = grant.clone();
    settle_grant(env, &mut preview, env.ledger().timestamp())?;
    Ok(preview)
}

#[contractimpl]
impl GrantContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        grant_token: Address,
        treasury: Address,
        oracle: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::GrantToken, &grant_token);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::Oracle, &oracle);
        env.storage().instance().set(&DataKey::GrantIds, &Vec::<u64>::new(&env));
        Ok(())
    }

    pub fn set_protocol_config(
        env: Env,
        sorosusu: Address,
        treasury: Address,
        sbt_minter: Address,
        debt_divert_bps: i128,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let config = ProtocolConfig {
            sorosusu_address: sorosusu,
            treasury_address: treasury,
            sbt_minter_address: sbt_minter,
            debt_divert_bps,
        };
        env.storage().instance().set(&DataKey::ProtocolConfig, &config);
        Ok(())
    }

    pub fn create_grant(
        env: Env,
        grant_id: u64,
        recipient: Address,
        total_amount: i128,
        flow_rate: i128,
        warmup_duration: u64,
        partner: Option<Address>,
        auto_debt_service: bool,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if total_amount <= 0 { return Err(Error::InvalidAmount); }
        if flow_rate <= 0 { return Err(Error::InvalidRate); }

        let key = DataKey::Grant(grant_id);
        if env.storage().instance().has(&key) {
            return Err(Error::GrantAlreadyExists);
        }

        let now = env.ledger().timestamp();
        let joint_info = partner.map(|p| JointGrantInfo { partner: p });

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
            joint_info,
            sorosusu_debt_service: auto_debt_service,
            total_volume_serviced: 0,
            start_time: now,
            warmup_duration,
        };

        env.storage().instance().set(&key, &grant);

        let mut ids = read_grant_ids(&env);
        ids.push_back(grant_id);
        env.storage().instance().set(&DataKey::GrantIds, &ids);

        let recipient_key = DataKey::RecipientGrants(recipient);
        let mut user_grants: Vec<u64> = env.storage().instance().get(&recipient_key).unwrap_or(vec![&env]);
        user_grants.push_back(grant_id);
        env.storage().instance().set(&recipient_key, &user_grants);

        Ok(())
    }

    pub fn propose_rate_change(env: Env, grant_id: u64, new_rate: i128) -> Result<(), Error> {
        require_admin_auth(&env)?;
        if new_rate < 0 { return Err(Error::InvalidRate); }

        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }

        let now = env.ledger().timestamp();
        settle_grant(&env, &mut grant, now)?;

        if grant.status != GrantStatus::Active {
            write_grant(&env, grant_id, &grant);
            return Err(Error::InvalidState);
        }

        let old_rate = grant.flow_rate;

        if new_rate < grant.flow_rate {
            grant.flow_rate = new_rate;
            grant.rate_updated_at = now;
            grant.pending_rate = 0;
            grant.effective_timestamp = 0;
        } else {
            grant.pending_rate = new_rate;
            grant.effective_timestamp = now.checked_add(RATE_INCREASE_TIMELOCK_SECS).ok_or(Error::MathOverflow)?;
        }

        write_grant(&env, grant_id, &grant);
        env.events().publish((symbol_short!("ratechg"), grant_id), (old_rate, new_rate, grant.effective_timestamp));
        Ok(())
    }

    pub fn withdraw(env: Env, grant_id: u64, amount: i128) -> Result<(), Error> {
        if amount <= 0 { return Err(Error::InvalidAmount); }
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status == GrantStatus::Cancelled { return Err(Error::InvalidState); }

        if let Some(ref joint) = grant.joint_info {
            grant.recipient.require_auth();
            joint.partner.require_auth();
        } else {
            grant.recipient.require_auth();
        }

        let now = env.ledger().timestamp();
        settle_grant(&env, &mut grant, now)?;

        if amount > grant.claimable { return Err(Error::InvalidAmount); }

        grant.claimable = grant.claimable.checked_sub(amount).ok_or(Error::MathOverflow)?;
        grant.withdrawn = grant.withdrawn.checked_add(amount).ok_or(Error::MathOverflow)?;
        grant.last_claim_time = now;

        if grant.withdrawn >= grant.total_amount {
            grant.status = GrantStatus::Completed;
            if let Ok(config) = read_config(&env) {
                mint_sbt(&env, &config, grant_id, &grant.recipient);
            }
        }

        let token = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token);
        client.transfer(&env.current_contract_address(), &grant.recipient, amount);

        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    pub fn slash_inactive_grant(env: Env, grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }

        let now = env.ledger().timestamp();
        settle_grant(&env, &mut grant, now)?;

        let inactive_secs = now.saturating_sub(grant.last_claim_time);
        if inactive_secs < INACTIVITY_THRESHOLD_SECS {
            return Err(Error::GrantNotInactive);
        }

        let remaining = grant.total_amount.checked_sub(grant.withdrawn).ok_or(Error::MathOverflow)?;
        grant.flow_rate = 0;
        grant.status = GrantStatus::Cancelled;
        write_grant(&env, grant_id, &grant);

        if remaining > 0 {
            let token = read_grant_token(&env)?;
            let treasury = read_treasury(&env)?;
            let client = token::Client::new(&env, &token);
            client.transfer(&env.current_contract_address(), &treasury, remaining);
        }
        Ok(())
    }

    pub fn split_and_separate(env: Env, grant_id: u64, new_grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        if let Some(joint) = grant.joint_info {
            grant.recipient.require_auth();
            joint.partner.require_auth();

            let now = env.ledger().timestamp();
            settle_grant(&env, &mut grant, now)?;

            let remaining_total = grant.total_amount.checked_sub(grant.withdrawn).ok_or(Error::MathOverflow)?;
            let half_remaining = remaining_total.checked_div(2).ok_or(Error::MathOverflow)?;
            let half_rate = grant.flow_rate.checked_div(2).ok_or(Error::MathOverflow)?;

            grant.total_amount = grant.withdrawn.checked_add(half_remaining).ok_or(Error::MathOverflow)?;
            grant.flow_rate = half_rate;
            grant.joint_info = None;
            write_grant(&env, grant_id, &grant);

            let partner_grant = Grant {
                recipient: joint.partner,
                total_amount: half_remaining,
                withdrawn: 0,
                claimable: 0,
                flow_rate: half_rate,
                last_update_ts: now,
                rate_updated_at: now,
                last_claim_time: now,
                pending_rate: 0,
                effective_timestamp: 0,
                status: GrantStatus::Active,
                joint_info: None,
                sorosusu_debt_service: false,
                total_volume_serviced: 0,
                start_time: now,
                warmup_duration: grant.warmup_duration,
            };
            write_grant(&env, new_grant_id, &partner_grant);

            let mut ids = read_grant_ids(&env);
            ids.push_back(new_grant_id);
            env.storage().instance().set(&DataKey::GrantIds, &ids);

            Ok(())
        } else {
            Err(Error::InvalidState)
        }
    }

    pub fn apply_kpi_multiplier(env: Env, grant_id: u64, multiplier: i128) -> Result<(), Error> {
        require_oracle_auth(&env)?;
        if multiplier <= 0 { return Err(Error::InvalidRate); }

        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }

        let now = env.ledger().timestamp();
        settle_grant(&env, &mut grant, now)?;

        let old_rate = grant.flow_rate;
        grant.flow_rate = grant.flow_rate.checked_mul(multiplier).ok_or(Error::MathOverflow)?;
        if grant.pending_rate > 0 {
            grant.pending_rate = grant.pending_rate.checked_mul(multiplier).ok_or(Error::MathOverflow)?;
        }

        write_grant(&env, grant_id, &grant);
        env.events().publish((symbol_short!("kpichg"), grant_id), (old_rate, grant.flow_rate));
        Ok(())
    }

    pub fn rescue_tokens(env: Env, token: Address, amount: i128, to: Address) -> Result<(), Error> {
        require_admin_auth(&env)?;
        if amount <= 0 { return Err(Error::InvalidAmount); }

        let grant_token = read_grant_token(&env)?;
        if token == grant_token {
            let allocated = total_allocated_funds(&env)?;
            let balance = token::Client::new(&env, &token).balance(&env.current_contract_address());
            if balance.checked_sub(amount).ok_or(Error::MathOverflow)? < allocated {
                return Err(Error::RescueWouldViolateAllocated);
            }
        }

        token::Client::new(&env, &token).transfer(&env.current_contract_address(), &to, amount);
        Ok(())
    }
}

fn total_allocated_funds(env: &Env) -> Result<i128, Error> {
    let mut total = 0_i128;
    let ids = read_grant_ids(env);
    for i in 0..ids.len() {
        let grant_id = ids.get(i).unwrap();
        let grant = read_grant(env, grant_id)?;
        if grant.status == GrantStatus::Active {
            let remaining = grant.total_amount.checked_sub(grant.withdrawn).ok_or(Error::MathOverflow)?;
            total = total.checked_add(remaining).ok_or(Error::MathOverflow)?;
        }
    }
    Ok(total)
}

mod test;
