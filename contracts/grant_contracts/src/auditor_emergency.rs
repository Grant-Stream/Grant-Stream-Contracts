#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec, symbol_short};

const EMERGENCY_PAUSE_DURATION: u64 = 7 * 24 * 60 * 60; // 7 days

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct AuditorConfig {
    pub auditors: Vec<Address>,
    pub required_signatures: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct PauseState {
    pub is_paused: bool,
    pub paused_at: u64,
    pub signers: Vec<Address>,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub enum AuditorDataKey {
    Config,
    State,
}

#[contract]
pub struct AuditorEmergencyProtocol;

#[contractimpl]
impl AuditorEmergencyProtocol {
    pub fn configure(env: Env, admin: Address, auditors: Vec<Address>, required_signatures: u32) {
        admin.require_auth();
        if required_signatures == 0 || required_signatures > auditors.len() { panic!("Invalid required signatures"); }
        let config = AuditorConfig { auditors, required_signatures };
        env.storage().instance().set(&AuditorDataKey::Config, &config);
    }

    pub fn sign_emergency_pause(env: Env, auditor: Address) -> bool {
        auditor.require_auth();
        let config: AuditorConfig = env.storage().instance().get(&AuditorDataKey::Config).expect("Not configured");
        if !config.auditors.contains(auditor.clone()) { panic!("Not an auditor"); }
        
        let mut state: PauseState = env.storage().instance().get(&AuditorDataKey::State).unwrap_or(PauseState { is_paused: false, paused_at: 0, signers: Vec::new(&env) });
        let now = env.ledger().timestamp();
        
        if state.is_paused && now > state.paused_at + EMERGENCY_PAUSE_DURATION {
            state.is_paused = false;
            state.signers = Vec::new(&env);
        } else if state.is_paused {
            panic!("Protocol already paused");
        }
        
        if !state.signers.contains(auditor.clone()) { state.signers.push_back(auditor); }
        
        if state.signers.len() >= config.required_signatures {
            state.is_paused = true;
            state.paused_at = now;
            env.events().publish((symbol_short!("aud_pause"),), (state.paused_at, state.signers.len()));
        }
        env.storage().instance().set(&AuditorDataKey::State, &state);
        state.is_paused
    }
}