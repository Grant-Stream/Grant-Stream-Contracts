#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env,
};

#[contract]
pub struct GrantContract;


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
    pub status: GrantStatus,
    pub joint_info: Option<JointGrantInfo>,    // Issue #223
    pub sorosusu_debt_service: bool,           // Issue #213
    pub total_volume_serviced: i128,           // Track for Issue #233
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Grant(u64),
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
}

fn read_admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)
}

fn require_admin_auth(env: &Env) -> Result<(), Error> {
    let admin = read_admin(env)?;
    admin.require_auth();
    Ok(())
}

fn read_grant(env: &Env, grant_id: u64) -> Result<Grant, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Grant(grant_id))
        .ok_or(Error::GrantNotFound)
}

fn write_grant(env: &Env, grant_id: u64, grant: &Grant) {
    env.storage().instance().set(&DataKey::Grant(grant_id), grant);
}

fn read_config(env: &Env) -> Result<ProtocolConfig, Error> {
    env.storage()
        .instance()
        .get(&DataKey::ProtocolConfig)
        .ok_or(Error::ConfigNotSet)
}

fn is_in_default(env: &Env, sorosusu: &Address, user: &Address) -> bool {
    // Interface with SoroSusu protocol
    // For this implementation, we assume a "get_default_status" method exists
    env.invoke_contract::<bool>(sorosusu, &symbol_short!("is_deflt"), soroban_sdk::vec![env, user.clone()])
}

fn mint_sbt(env: &Env, config: &ProtocolConfig, grant_id: u64, recipient: &Address) {
    // Mint Soulbound Token on completion (Issue #232)
    let _: () = env.invoke_contract(
        &config.sbt_minter_address,
        &symbol_short!("mint_sbt"),
        soroban_sdk::vec![env, grant_id, recipient.clone()],
    );
}

const TAX_THRESHOLD: i128 = 100_000_0000000; // $100,000 in 7-decimal places
const TAX_BPS: i128 = 1; // 0.01%



fn settle_grant(env: &Env, grant: &mut Grant, now: u64) -> Result<(), Error> {
    if now < grant.last_update_ts {
        return Err(Error::InvalidState);
    }

    let elapsed = now - grant.last_update_ts;
    grant.last_update_ts = now;

    if grant.status != GrantStatus::Active || elapsed == 0 || grant.flow_rate == 0 {
        return Ok(());
    }

    let elapsed_i128 = i128::from(elapsed);
    let accrued = grant
        .flow_rate
        .checked_mul(elapsed_i128)
        .ok_or(Error::MathOverflow)?;

    let accounted = grant
        .withdrawn
        .checked_add(grant.claimable)
        .ok_or(Error::MathOverflow)?;

    let remaining = grant
        .total_amount
        .checked_sub(accounted)
        .ok_or(Error::MathOverflow)?;

    let delta = if accrued > remaining {
        remaining
    } else {
        accrued
    };

    if delta == 0 {
        return Ok(());
    }

    let config = read_config(env)?;
    let mut net_delta = delta;

    // Issue #233: Sustainability Tax (0.01% if > $100k)
    if grant.total_amount >= TAX_THRESHOLD {
        let tax = delta.checked_mul(TAX_BPS).unwrap().checked_div(10000).unwrap();
        if tax > 0 {
            // Transfer tax straight to treasury
            env.invoke_contract(
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
                // Transfer debt service to SoroSusu
                env.invoke_contract(
                    &config.sorosusu_address,
                    &symbol_short!("repay"),
                    soroban_sdk::vec![env, grant.recipient.clone(), debt_service],
                );
                net_delta = net_delta.checked_sub(debt_service).ok_or(Error::MathOverflow)?;
            }
        }
    }

    grant.claimable = grant
        .claimable
        .checked_add(net_delta)
        .ok_or(Error::MathOverflow)?;

    grant.total_volume_serviced = grant.total_volume_serviced.checked_add(delta).ok_or(Error::MathOverflow)?;

    let new_accounted = grant
        .withdrawn
        .checked_add(grant.claimable)
        .ok_or(Error::MathOverflow)?;

    if new_accounted >= grant.total_amount {
        grant.status = GrantStatus::Completed;
    }

    Ok(())
}

fn preview_grant_at_now(env: &Env, grant: &Grant) -> Result<Grant, Error> {
    let mut preview = grant.clone();
    settle_grant(env, &mut preview, env.ledger().timestamp())?;
    Ok(preview)
}

#[contractimpl]
impl GrantContract {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
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
        partner: Option<Address>, // For joint-grant
        auto_debt_service: bool,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if total_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if flow_rate < 0 {
            return Err(Error::InvalidRate);
        }

        let key = DataKey::Grant(grant_id);
        if env.storage().instance().has(&key) {
            return Err(Error::GrantAlreadyExists);
        }

        let joint_info = partner.map(|p| JointGrantInfo { partner: p });

        let now = env.ledger().timestamp();
        let grant = Grant {
            recipient,
            total_amount,
            withdrawn: 0,
            claimable: 0,
            flow_rate,
            last_update_ts: now,
            rate_updated_at: now,
            status: GrantStatus::Active,
            joint_info,
            sorosusu_debt_service: auto_debt_service,
            total_volume_serviced: 0,
        };

        env.storage().instance().set(&key, &grant);
        Ok(())
    }

    pub fn cancel_grant(env: Env, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;

        if grant.status != GrantStatus::Active {
            return Err(Error::InvalidState);
        }

        settle_grant(&env, &mut grant, env.ledger().timestamp())?;
        grant.flow_rate = 0;
        grant.status = GrantStatus::Cancelled;
        write_grant(&env, grant_id, &grant);

        Ok(())
    }

    pub fn get_grant(env: Env, grant_id: u64) -> Result<Grant, Error> {
        let grant = read_grant(&env, grant_id)?;
        preview_grant_at_now(&env, &grant)
    }

    pub fn claimable(env: Env, grant_id: u64) -> Result<i128, Error> {
        let grant = read_grant(&env, grant_id)?;
        let preview = preview_grant_at_now(&env, &grant)?;
        Ok(preview.claimable)
    }

    pub fn withdraw(env: Env, grant_id: u64, amount: i128) -> Result<(), Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut grant = read_grant(&env, grant_id)?;

        if grant.status == GrantStatus::Cancelled {
            return Err(Error::InvalidState);
        }

        // Issue #223: Dual-Signatures for Joint Grants
        if let Some(ref joint) = grant.joint_info {
            grant.recipient.require_auth();
            joint.partner.require_auth();
        } else {
            grant.recipient.require_auth();
        }

        settle_grant(&env, &mut grant, env.ledger().timestamp())?;

        if amount > grant.claimable {
            return Err(Error::InvalidAmount);
        }

        grant.claimable = grant
            .claimable
            .checked_sub(amount)
            .ok_or(Error::MathOverflow)?;
        grant.withdrawn = grant
            .withdrawn
            .checked_add(amount)
            .ok_or(Error::MathOverflow)?;

        if grant.withdrawn >= grant.total_amount {
            grant.status = GrantStatus::Completed;
            let config = read_config(&env)?;
            mint_sbt(&env, &config, grant_id, &grant.recipient);
        }

        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    pub fn split_and_separate(env: Env, grant_id: u64, new_grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        
        // Both parties must agree to split
        if let Some(joint) = grant.joint_info {
            grant.recipient.require_auth();
            joint.partner.require_auth();

            settle_grant(&env, &mut grant, env.ledger().timestamp())?;

            let remaining_total = grant.total_amount.checked_sub(grant.withdrawn).ok_or(Error::MathOverflow)?;
            let half_remaining = remaining_total.checked_div(2).ok_or(Error::MathOverflow)?;
            let half_rate = grant.flow_rate.checked_div(2).ok_or(Error::MathOverflow)?;

            // Update original grant to be recipient's independent flow
            grant.total_amount = grant.withdrawn.checked_add(half_remaining).ok_or(Error::MathOverflow)?;
            grant.flow_rate = half_rate;
            grant.joint_info = None;
            write_grant(&env, grant_id, &grant);

            // Create new grant for the partner
            let now = env.ledger().timestamp();
            let partner_grant = Grant {
                recipient: joint.partner,
                total_amount: half_remaining,
                withdrawn: 0,
                claimable: 0,
                flow_rate: half_rate,
                last_update_ts: now,
                rate_updated_at: now,
                status: GrantStatus::Active,
                joint_info: None,
                sorosusu_debt_service: false,
                total_volume_serviced: 0,
            };
            write_grant(&env, new_grant_id, &partner_grant);

            Ok(())
        } else {
            Err(Error::InvalidState)
        }
    }

    pub fn update_rate(env: Env, grant_id: u64, new_rate: i128) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if new_rate < 0 {
            return Err(Error::InvalidRate);
        }

        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active {
            return Err(Error::InvalidState);
        }

        let old_rate = grant.flow_rate;

        settle_grant(&env, &mut grant, env.ledger().timestamp())?;

        if grant.status != GrantStatus::Active {
            write_grant(&env, grant_id, &grant);
            return Err(Error::InvalidState);
        }

        grant.flow_rate = new_rate;
        grant.rate_updated_at = grant.last_update_ts;

        write_grant(&env, grant_id, &grant);

        env.events().publish(
            (symbol_short!("rateupdt"), grant_id),
            (old_rate, new_rate, grant.rate_updated_at),
        );

        Ok(())
    }
}

mod test;
