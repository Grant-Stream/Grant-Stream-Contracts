#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

const DECAY_INTERVAL_SECS: u64 = 365 * 24 * 60 * 60; // 12 months
const DECAY_RATE_BPS: u32 = 1000; // 10% decay per year

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct IdentityReputation {
    pub grantee: Address,
    pub trust_score: u32, // 0 to 10000 (0.00% to 100.00%)
    pub last_milestone_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub enum ReputationDataKey {
    Reputation(Address),
}

#[contract]
pub struct ReputationDecayLogic;

#[contractimpl]
impl ReputationDecayLogic {
    pub fn initialize_reputation(env: Env, grantee: Address, initial_score: u32) {
        let rep = IdentityReputation {
            grantee: grantee.clone(),
            trust_score: initial_score.min(10000),
            last_milestone_at: env.ledger().timestamp(),
        };
        env.storage().instance().set(&ReputationDataKey::Reputation(grantee), &rep);
    }

    pub fn record_milestone_completion(env: Env, grantee: Address) {
        let mut rep: IdentityReputation = env.storage().instance()
            .get(&ReputationDataKey::Reputation(grantee.clone()))
            .unwrap_or(IdentityReputation {
                grantee: grantee.clone(),
                trust_score: 5000,
                last_milestone_at: env.ledger().timestamp(),
            });

        let current_score = Self::calculate_current_score(env.clone(), grantee.clone());
        rep.trust_score = current_score.saturating_add(500).min(10000);
        rep.last_milestone_at = env.ledger().timestamp();
        
        env.storage().instance().set(&ReputationDataKey::Reputation(grantee), &rep);
    }

    pub fn calculate_current_score(env: Env, grantee: Address) -> u32 {
        if let Some(rep) = env.storage().instance().get::<_, IdentityReputation>(&ReputationDataKey::Reputation(grantee)) {
            let now = env.ledger().timestamp();
            if now > rep.last_milestone_at + DECAY_INTERVAL_SECS {
                let intervals = (now - rep.last_milestone_at) / DECAY_INTERVAL_SECS;
                let mut decayed_score = rep.trust_score;
                for _ in 0..intervals {
                    decayed_score = decayed_score.saturating_sub((decayed_score * DECAY_RATE_BPS) / 10000);
                }
                return decayed_score;
            }
            return rep.trust_score;
        }
        0
    }
}