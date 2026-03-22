#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
    Symbol, vec, IntoVal, String, Bytes, Map,
};

// --- Constants ---
pub const SCALING_FACTOR: i128 = 10_000_000; // 1e7
const XLM_DECIMALS: u32 = 7;
const RENT_RESERVE_XLM: i128 = 5 * 10i128.pow(XLM_DECIMALS);
const RATE_INCREASE_TIMELOCK_SECS: u64 = 48 * 60 * 60;
const INACTIVITY_THRESHOLD_SECS: u64 = 90 * 24 * 60 * 60;
const NFT_SUPPLY: i128 = 1000000; // Max NFT supply for completion certificates

// --- Submodules ---
// Submodules removed for consolidation and to fix compilation errors.
// Core logic is now in this file.

// --- Types ---

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum GrantStatus {
    Active,
    Paused,
    Completed,
    Cancelled,
    RageQuitted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum StreamType {
    FixedAmount,
    FixedEndDate,
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
    pub redirect: Option<Address>,
    pub stream_type: StreamType,
    pub start_time: u64,
    pub warmup_duration: u64,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    GrantToken,
    GrantIds,
    Treasury,
    Oracle,
    NativeToken,
    Grant(u64),
    RecipientGrants(Address),
    NFTOwner(i128),
    NFTTokenCount,
    NFTApprovals(i128),
    CompletionNFT(u64), // Maps grant_id to nft_token_id
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
    InsufficientReserve = 10,
    RescueWouldViolateAllocated = 11,
    GranteeMismatch = 12,
    GrantNotInactive = 13,
    NFTAlreadyMinted = 14,
    NFTMaxSupplyReached = 15,
    NFTNotFound = 16,
}

// --- Internal Helpers ---

fn read_admin(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotInitialized)
}

fn read_oracle(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Oracle).ok_or(Error::NotInitialized)
}

fn require_admin_auth(env: &Env) -> Result<(), Error> {
    read_admin(env)?.require_auth();
    Ok(())
}

fn require_oracle_auth(env: &Env) -> Result<(), Error> {
    read_oracle(env)?.require_auth();
    Ok(())
}

fn read_grant(env: &Env, grant_id: u64) -> Result<Grant, Error> {
    env.storage().instance().get(&DataKey::Grant(grant_id)).ok_or(Error::GrantNotFound)
}

fn write_grant(env: &Env, grant_id: u64, grant: &Grant) {
    env.storage().instance().set(&DataKey::Grant(grant_id), grant);
}

fn read_grant_token(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::GrantToken).ok_or(Error::NotInitialized)
}

fn read_treasury(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Treasury).ok_or(Error::NotInitialized)
}

fn read_grant_ids(env: &Env) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&DataKey::GrantIds)
        .unwrap_or_else(|| Vec::new(env))
}

// NFT Helper Functions
fn read_nft_token_count(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::NFTTokenCount).unwrap_or(0)
}

fn write_nft_token_count(env: &Env, count: i128) {
    env.storage().instance().set(&DataKey::NFTTokenCount, &count);
}

fn read_nft_owner(env: &Env, token_id: i128) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::NFTOwner(token_id)).ok_or(Error::NFTNotFound)
}

fn write_nft_owner(env: &Env, token_id: i128, owner: &Address) {
    env.storage().instance().set(&DataKey::NFTOwner(token_id), owner);
}

fn read_completion_nft(env: &Env, grant_id: u64) -> Result<i128, Error> {
    env.storage().instance().get(&DataKey::CompletionNFT(grant_id)).ok_or(Error::NFTNotFound)
}

fn write_completion_nft(env: &Env, grant_id: u64, token_id: i128) {
    env.storage().instance().set(&DataKey::CompletionNFT(grant_id), &token_id);
}

fn generate_completion_metadata(
    env: &Env,
    grant_id: u64,
    recipient: &Address,
    total_amount: i128,
    token_symbol: &str,
    dao_name: &str,
    repo_url: &str,
) -> String {
    let completion_date = env.ledger().timestamp();
    let contract_address = env.current_contract_address();
    
    // Create JSON metadata following SEP-0039 standards
    let metadata = format!(
        r#"{{
  "name": "Stellar Grant Completion Certificate",
  "description": "Certificate of completion for Grant #{}. This NFT represents successful delivery of a funded project on the Stellar network.",
  "image": "ipfs://QmCompletionCertificateImageHash",
  "external_url": "https://grant-platform.xyz/grants/{}",
  "attributes": [
    {{
      "trait_type": "Grant ID",
      "value": "{}"
    }},
    {{
      "trait_type": "Funding DAO",
      "value": "{}"
    }},
    {{
      "trait_type": "Total Amount",
      "value": "{}"
    }},
    {{
      "trait_type": "Token",
      "value": "{}"
    }},
    {{
      "trait_type": "Completion Date",
      "value": "{}"
    }},
    {{
      "trait_type": "Recipient",
      "value": "{}"
    }}
  ],
  "issuer": "{}",
  "code": "GCC{}",
  "project_repo": "{}",
  "certificate_type": "grant_completion"
}}"#,
        grant_id,
        grant_id,
        grant_id,
        dao_name,
        total_amount,
        token_symbol,
        completion_date,
        recipient,
        contract_address,
        grant_id,
        repo_url
    );
    
    String::from_str(env, &metadata)
}

fn mint_completion_certificate(
    env: &Env,
    grant_id: u64,
    recipient: &Address,
    total_amount: i128,
    token_symbol: &str,
    dao_name: &str,
    repo_url: &str,
) -> Result<i128, Error> {
    // Check if NFT already minted for this grant
    if env.storage().instance().has(&DataKey::CompletionNFT(grant_id)) {
        return Err(Error::NFTAlreadyMinted);
    }
    
    // Get current token count and check supply limit
    let mut token_count = read_nft_token_count(env);
    if token_count >= NFT_SUPPLY {
        return Err(Error::NFTMaxSupplyReached);
    }
    
    // Increment token count for new NFT
    token_count = token_count.checked_add(1).ok_or(Error::MathOverflow)?;
    write_nft_token_count(env, token_count);
    
    // Set NFT owner
    write_nft_owner(env, token_count, recipient);
    
    // Map grant_id to nft_token_id
    write_completion_nft(env, grant_id, token_count);
    
    // Generate metadata (in production, this would be uploaded to IPFS)
    let _metadata = generate_completion_metadata(
        env,
        grant_id,
        recipient,
        total_amount,
        token_symbol,
        dao_name,
        repo_url,
    );
    
    // Publish mint event
    env.events().publish(
        (symbol_short!("completion_nft_minted"), grant_id),
        (recipient, token_count, total_amount),
    );
    
    Ok(token_count)
}

fn total_allocated_funds(env: &Env) -> Result<i128, Error> {
    let mut total = 0_i128;
    let ids = read_grant_ids(env);
    for i in 0..ids.len() {
        let grant_id = ids.get(i).unwrap();
        if let Some(grant) = env.storage().instance().get::<_, Grant>(&DataKey::Grant(grant_id)) {
            if grant.status == GrantStatus::Active || grant.status == GrantStatus::Paused {
                let remaining = grant.total_amount
                    .checked_sub(grant.withdrawn)
                    .ok_or(Error::MathOverflow)?;
                total = total.checked_add(remaining).ok_or(Error::MathOverflow)?;
            }
        }
    }
    Ok(total)
}

fn calculate_warmup_multiplier(grant: &Grant, now: u64) -> i128 {
    if grant.warmup_duration == 0 {
        return 10000; // 100% in basis points
    }

    let warmup_end = grant.start_time + grant.warmup_duration;

    if now >= warmup_end {
        return 10000; 
    }

    if now <= grant.start_time {
        return 2500; // 25% at start
    }

    let elapsed_warmup = now - grant.start_time;
    let progress = ((elapsed_warmup as i128) * 10000) / (grant.warmup_duration as i128);

    // 25% + (75% * progress)
    2500 + (7500 * progress) / 10000
}

fn settle_grant(env: &Env, grant: &mut Grant, grant_id: u64, now: u64) -> Result<(), Error> {
    if now < grant.last_update_ts { return Err(Error::InvalidState); }
    
    let elapsed = now - grant.last_update_ts;
    if elapsed == 0 {
        return Ok(());
    }

    let was_completed = grant.status == GrantStatus::Completed;

    if grant.status == GrantStatus::Active {
        // Handle pending rate increases first
        if grant.pending_rate > grant.flow_rate && grant.effective_timestamp != 0 && now >= grant.effective_timestamp {
            let switch_ts = grant.effective_timestamp;
            // Settle up to switch_ts at old rate
            let pre_elapsed = switch_ts - grant.last_update_ts;
            let pre_accrued = calculate_accrued(grant, pre_elapsed, switch_ts)?;
            grant.claimable = grant.claimable.checked_add(pre_accrued).ok_or(Error::MathOverflow)?;
            
            // Apply new rate
            grant.flow_rate = grant.pending_rate;
            grant.rate_updated_at = switch_ts;
            grant.pending_rate = 0;
            grant.effective_timestamp = 0;
            grant.last_update_ts = switch_ts;
            
            // Recalculate remaining elapsed
            let post_elapsed = now - switch_ts;
            let post_accrued = calculate_accrued(grant, post_elapsed, now)?;
            grant.claimable = grant.claimable.checked_add(post_accrued).ok_or(Error::MathOverflow)?;
        } else {
            let accrued = calculate_accrued(grant, elapsed, now)?;
            grant.claimable = grant.claimable.checked_add(accrued).ok_or(Error::MathOverflow)?;
        }
    }

    let total_accounted = grant.withdrawn.checked_add(grant.claimable).ok_or(Error::MathOverflow)?;
    if total_accounted >= grant.total_amount {
        grant.claimable = grant.total_amount - grant.withdrawn;
        grant.status = GrantStatus::Completed;
        
        // Mint completion certificate if this is the first time completing
        if !was_completed {
            let _token_id = mint_completion_certificate(
                env,
                grant_id,
                &grant.recipient,
                grant.total_amount,
                "USDC", // Default token symbol - should be parameterized
                "Stellar DAO", // Default DAO name - should be parameterized
                "https://github.com/example/repo", // Default repo - should be parameterized
            )?;
        }
    }

    grant.last_update_ts = now;
    Ok(())
}

fn calculate_accrued(grant: &Grant, elapsed: u64, now: u64) -> Result<i128, Error> {
    let elapsed_i128 = i128::from(elapsed);
    let base_accrued = grant.flow_rate.checked_mul(elapsed_i128).ok_or(Error::MathOverflow)?;

    let multiplier = calculate_warmup_multiplier(grant, now);
    let accrued = base_accrued
        .checked_mul(multiplier)
        .ok_or(Error::MathOverflow)?
        .checked_div(10000)
        .ok_or(Error::MathOverflow)?;

    Ok(accrued)
}

// --- Contract Implementation ---

#[contract]
pub struct GrantContract;

#[contractimpl]
impl GrantContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        grant_token: Address,
        treasury: Address,
        oracle: Address,
        native_token: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::GrantToken, &grant_token);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::Oracle, &oracle);
        env.storage().instance().set(&DataKey::NativeToken, &native_token);
        env.storage().instance().set(&DataKey::GrantIds, &Vec::<u64>::new(&env));
        Ok(())
    }

    pub fn create_grant(
        env: Env,
        grant_id: u64,
        recipient: Address,
        total_amount: i128,
        flow_rate: i128,
        warmup_duration: u64
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if total_amount <= 0 || flow_rate < 0 {
            return Err(Error::InvalidAmount);
        }

        let key = DataKey::Grant(grant_id);
        if env.storage().instance().has(&key) {
            return Err(Error::GrantAlreadyExists);
        }

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
            redirect: None,
            stream_type: StreamType::FixedAmount,
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

    pub fn withdraw(env: Env, grant_id: u64, amount: i128) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        grant.recipient.require_auth();

        if grant.status == GrantStatus::Cancelled || grant.status == GrantStatus::RageQuitted {
            return Err(Error::InvalidState);
        }

        settle_grant(&env, &mut grant, grant_id, env.ledger().timestamp())?;

        if amount > grant.claimable {
            return Err(Error::InvalidAmount);
        }

        grant.claimable = grant.claimable.checked_sub(amount).ok_or(Error::MathOverflow)?;
        grant.withdrawn = grant.withdrawn.checked_add(amount).ok_or(Error::MathOverflow)?;
        grant.last_claim_time = env.ledger().timestamp();

        write_grant(&env, grant_id, &grant);

        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        let target = grant.redirect.unwrap_or(grant.recipient.clone());
        client.transfer(&env.current_contract_address(), &target, &amount);

        try_call_on_withdraw(&env, &grant.recipient, grant_id, amount);

        Ok(())
    }

    pub fn pause_stream(env: Env, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }
        
        settle_grant(&env, &mut grant, grant_id, env.ledger().timestamp())?;
        grant.status = GrantStatus::Paused;
        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    pub fn resume_stream(env: Env, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Paused { return Err(Error::InvalidState); }

        grant.status = GrantStatus::Active;
        grant.last_update_ts = env.ledger().timestamp();
        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    pub fn propose_rate_change(env: Env, grant_id: u64, new_rate: i128) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }
        if new_rate < 0 { return Err(Error::InvalidRate); }

        settle_grant(&env, &mut grant, grant_id, env.ledger().timestamp())?;
        
        let old_rate = grant.flow_rate;
        if new_rate > old_rate {
            grant.pending_rate = new_rate;
            grant.effective_timestamp = env.ledger().timestamp() + RATE_INCREASE_TIMELOCK_SECS;
        } else {
            grant.flow_rate = new_rate;
            grant.rate_updated_at = env.ledger().timestamp();
            grant.pending_rate = 0;
            grant.effective_timestamp = 0;
        }

        write_grant(&env, grant_id, &grant);
        env.events().publish((symbol_short!("rateupdt"), grant_id), (old_rate, new_rate));
        Ok(())
    }

    pub fn apply_kpi_multiplier(env: Env, grant_id: u64, multiplier: i128) -> Result<(), Error> {
        require_oracle_auth(&env)?;
        if multiplier <= 0 { return Err(Error::InvalidRate); }

        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }

        settle_grant(&env, &mut grant, grant_id, env.ledger().timestamp())?;
        
        let old_rate = grant.flow_rate;
        grant.flow_rate = grant.flow_rate.checked_mul(multiplier).ok_or(Error::MathOverflow)? / 10000;
        grant.rate_updated_at = env.ledger().timestamp();

        write_grant(&env, grant_id, &grant);
        env.events().publish((symbol_short!("kpimul"), grant_id), (old_rate, grant.flow_rate, multiplier));
        Ok(())
    }

    pub fn rage_quit(env: Env, grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        grant.recipient.require_auth();

        if grant.status != GrantStatus::Paused { return Err(Error::InvalidState); }

        settle_grant(&env, &mut grant, grant_id, env.ledger().timestamp())?;
        
        let claim_amount = grant.claimable;
        grant.claimable = 0;
        grant.withdrawn = grant.withdrawn.checked_add(claim_amount).ok_or(Error::MathOverflow)?;
        grant.status = GrantStatus::RageQuitted;
        
        let remaining = grant.total_amount.checked_sub(grant.withdrawn).ok_or(Error::MathOverflow)?;
        write_grant(&env, grant_id, &grant);

        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        client.transfer(&env.current_contract_address(), &grant.recipient, &claim_amount);

        if remaining > 0 {
            let treasury = read_treasury(&env)?;
            client.transfer(&env.current_contract_address(), &treasury, &remaining);
        }

        Ok(())
    }

    pub fn rescue_tokens(env: Env, token_address: Address, amount: i128, to: Address) -> Result<(), Error> {
        require_admin_auth(&env)?;
        if amount <= 0 { return Err(Error::InvalidAmount); }

        let client = token::Client::new(&env, &token_address);
        let balance = client.balance(&env.current_contract_address());

        let total_allocated = if token_address == read_grant_token(&env)? {
            total_allocated_funds(&env)?
        } else {
            0
        };

        if balance.checked_sub(amount).ok_or(Error::MathOverflow)? < total_allocated {
            return Err(Error::RescueWouldViolateAllocated);
        }

        client.transfer(&env.current_contract_address(), &to, &amount);
        Ok(())
    }

    pub fn get_grant(env: Env, grant_id: u64) -> Result<Grant, Error> {
        read_grant(&env, grant_id)
    }

    pub fn claimable(env: Env, grant_id: u64) -> i128 {
        if let Ok(mut grant) = read_grant(&env, grant_id) {
            let _ = settle_grant(&env, &mut grant, grant_id, env.ledger().timestamp());
            grant.claimable
        } else {
            0
        }
    }

    // NFT-related functions
    pub fn nft_owner_of(env: Env, token_id: i128) -> Result<Address, Error> {
        read_nft_owner(&env, token_id)
    }

    pub fn nft_token_count(env: Env) -> i128 {
        read_nft_token_count(&env)
    }

    pub fn completion_nft_token_id(env: Env, grant_id: u64) -> Result<i128, Error> {
        read_completion_nft(&env, grant_id)
    }

    pub fn nft_metadata(
        env: Env,
        grant_id: u64,
        recipient: Address,
        total_amount: i128,
        token_symbol: String,
        dao_name: String,
        repo_url: String,
    ) -> String {
        generate_completion_metadata(
            &env,
            grant_id,
            &recipient,
            total_amount,
            &token_symbol.to_string(),
            &dao_name.to_string(),
            &repo_url.to_string(),
        )
    }
}

fn try_call_on_withdraw(env: &Env, recipient: &Address, grant_id: u64, amount: i128) {
    let args = (grant_id, amount).into_val(env);
    let _ = env.try_invoke_contract::<soroban_sdk::Val, soroban_sdk::Error>(
        recipient,
        &Symbol::new(env, "on_withdraw"),
        args,
    );
}

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_nft;
