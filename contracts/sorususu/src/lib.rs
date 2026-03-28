#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, contracterror, symbol_short,
    token, Address, Env, Vec, Map, String, Symbol, IntoVal,
};

// Constants
const ROUND_DURATION: u64 = 7 * 24 * 60 * 60; // 7 days default round
const MIN_MEMBERS: u32 = 3;
const MAX_MEMBERS: u32 = 50;
const GAS_BOUNTY_BPS: u32 = 10; // 0.1% of platform fee as gas bounty (in basis points)
const PLATFORM_FEE_BPS: u32 = 50; // 0.5% platform fee (in basis points)
const RECOVERY_SURCHARGE_BPS: u32 = 500; // 5% recovery surcharge for deficit (in basis points)
const VOTING_PERIOD: u64 = 3 * 24 * 60 * 60; // 3 days voting period
const DEFICIT_VOTING_THRESHOLD: u32 = 6600; // 66% approval to skip payout (in basis points)
const MIN_RELIABILITY_SCORE: u32 = 900; // Minimum score for grant priority

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Member {
    pub address: Address,
    pub contribution: i128,
    pub reliability_score: u32,
    pub rounds_participated: u32,
    pub rounds_won: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Round {
    pub round_id: u32,
    pub members: Vec<Address>,
    pub total_pot: i128,
    pub winner: Option<Address>,
    pub status: RoundStatus,
    pub started_at: u64,
    pub finalized_at: Option<u64>,
    pub finalized_by: Option<Address>,
    pub gas_bounty_paid: i128,
    pub deficit_amount: i128,
    pub recovery_surcharge_per_member: i128,
    pub deficit_vote_passed: bool,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoundStatus {
    Active,
    Finalized,
    DeficitPaused,
    Skipped,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeficitVote {
    pub round_id: u32,
    pub votes_for_skip: u32,
    pub votes_against_skip: u32,
    pub total_voting_power: u32,
    pub voting_deadline: u64,
    pub executed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrantPriorityCache {
    pub user: Address,
    pub reliability_score: u32,
    pub priority_enabled: bool,
    pub last_verified: u64,
    pub sorususu_contract: Address,
}

#[contracttype]
pub enum DataKey {
    Admin,
    PlatformFeeTreasury,
    GrantStreamContract, // Address of Grant-Stream contract for interop
    Member(Address),
    MemberCount,
    CurrentRound,
    RoundHistory(u32),
    DeficitVote(u32),
    TotalReliabilityScore(Address),
    PlatformFeePool,
}

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum SusuError {
    NotInitialized = 1,
    NotAuthorized = 2,
    InvalidAmount = 3,
    InvalidMember = 4,
    RoundNotFound = 5,
    RoundAlreadyFinalized = 6,
    InsufficientFunds = 7,
    MemberLimitReached = 8,
    TooFewMembers = 9,
    DeficitDetected = 10,
    VotingPeriodActive = 11,
    VotingPeriodEnded = 12,
    ThresholdNotMet = 13,
    InvalidReliabilityScore = 14,
    GrantStreamContractNotSet = 15,
    MathOverflow = 16,
}

#[contract]
pub struct SoroSusuContract;

#[contractimpl]
impl SoroSusuContract {
    /// Initialize the SoroSusu contract
    pub fn initialize(
        env: Env,
        admin: Address,
        platform_fee_treasury: Address,
    ) -> Result<(), SusuError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(SusuError::AlreadyInitialized);
        }
        
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::PlatformFeeTreasury, &platform_fee_treasury);
        env.storage().instance().set(&DataKey::MemberCount, &0u32);
        env.storage().instance().set(&DataKey::CurrentRound, &0u32);
        env.storage().instance().set(&DataKey::PlatformFeePool, &0i128);
        
        env.events().publish(
            (symbol_short!("sorususu_init"),),
            (admin, platform_fee_treasury),
        );
        
        Ok(())
    }

    /// Register a new member
    pub fn register_member(env: Env, member: Address, initial_contribution: i128) -> Result<(), SusuError> {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(SusuError::NotInitialized)?;
        admin.require_auth();
        
        if initial_contribution <= 0 {
            return Err(SusuError::InvalidAmount);
        }
        
        let member_count: u32 = env.storage().instance().get(&DataKey::MemberCount).unwrap_or(0);
        if member_count >= MAX_MEMBERS {
            return Err(SusuError::MemberLimitReached);
        }
        
        let member_data = Member {
            address: member.clone(),
            contribution: initial_contribution,
            reliability_score: 1000, // Start with perfect score
            rounds_participated: 0,
            rounds_won: 0,
        };
        
        env.storage().instance().set(&DataKey::Member(member), &member_data);
        env.storage().instance().set(&DataKey::MemberCount, &(member_count + 1));
        
        env.events().publish(
            (symbol_short!("member_registered"),),
            (member, initial_contribution),
        );
        
        Ok(())
    }

    /// Start a new round
    pub fn start_round(env: Env, members: Vec<Address>) -> Result<u32, SusuError> {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(SusuError::NotInitialized)?;
        admin.require_auth();
        
        if members.len() < MIN_MEMBERS {
            return Err(SusuError::TooFewMembers);
        }
        
        let current_round_id: u32 = env.storage().instance().get(&DataKey::CurrentRound).unwrap_or(0);
        let new_round_id = current_round_id + 1;
        
        // Calculate total pot from member contributions
        let mut total_pot = 0i128;
        for member_addr in members.iter() {
            let member: Member = env.storage().instance()
                .get(&DataKey::Member(member_addr))
                .ok_or(SusuError::InvalidMember)?;
            total_pot += member.contribution;
        }
        
        let round = Round {
            round_id: new_round_id,
            members: members.clone(),
            total_pot,
            winner: None,
            status: RoundStatus::Active,
            started_at: env.ledger().timestamp(),
            finalized_at: None,
            finalized_by: None,
            gas_bounty_paid: 0,
            deficit_amount: 0,
            recovery_surcharge_per_member: 0,
            deficit_vote_passed: false,
        };
        
        env.storage().instance().set(&DataKey::CurrentRound, &new_round_id);
        env.storage().instance().set(&DataKey::RoundHistory(new_round_id), &round);
        
        env.events().publish(
            (symbol_short!("round_started"),),
            (new_round_id, members.len(), total_pot),
        );
        
        Ok(new_round_id)
    }

    /// TASK 1: Finalize round with gas bounty incentive
    /// Anyone can call this to move the pot to the winner and receive a gas rebate
    pub fn finalize_round(env: Env, round_id: u32, winner: Address) -> Result<(), SusuError> {
        // No auth required - this is permissionless for decentralized maintenance
        
        let mut round: Round = env.storage().instance()
            .get(&DataKey::RoundHistory(round_id))
            .ok_or(SusuError::RoundNotFound)?;
        
        if round.status != RoundStatus::Active {
            return Err(SusuError::RoundAlreadyFinalized);
        }
        
        // Check if round duration has passed
        let now = env.ledger().timestamp();
        if now < round.started_at + ROUND_DURATION {
            return Err(SusuError::VotingPeriodActive);
        }
        
        // Check for clawback deficit (TASK 2 integration)
        let token_address: Address = env.storage().instance()
            .get(&DataKey::GrantToken)
            .ok_or(SusuError::NotInitialized)?;
        let token_client = token::Client::new(&env, &token_address);
        let contract_balance = token_client.balance(&env.current_contract_address());
        
        if contract_balance < round.total_pot {
            // Deficit detected - activate deficit resolution (TASK 2)
            return Self::activate_deficit_resolution(&env, round_id, contract_balance)?;
        }
        
        // Calculate platform fee
        let platform_fee = (round.total_pot * PLATFORM_FEE_BPS as i128) / 10000;
        let winner_amount = round.total_pot - platform_fee;
        
        // Calculate gas bounty for the caller
        let caller = env.invoker();
        let gas_bounty = (platform_fee * GAS_BOUNTY_BPS as i128) / 10000;
        let net_fee = platform_fee - gas_bounty;
        
        // Transfer winner amount
        token_client.transfer(&env.current_contract_address(), &winner, &winner_amount);
        
        // Transfer platform fee to treasury
        let treasury: Address = env.storage().instance().get(&DataKey::PlatformFeeTreasury).ok_or(SusuError::NotInitialized)?;
        token_client.transfer(&env.current_contract_address(), &treasury, &net_fee);
        
        // Pay gas bounty to caller
        if gas_bounty > 0 {
            token_client.transfer(&env.current_contract_address(), &caller, &gas_bounty);
        }
        
        // Update round state
        round.winner = Some(winner.clone());
        round.status = RoundStatus::Finalized;
        round.finalized_at = Some(now);
        round.finalized_by = Some(caller.clone());
        round.gas_bounty_paid = gas_bounty;
        
        env.storage().instance().set(&DataKey::RoundHistory(round_id), &round);
        
        // Update winner's reliability score
        Self::update_member_reliability(&env, &winner, true)?;
        
        env.events().publish(
            (symbol_short!("round_finalized"),),
            (round_id, winner, caller, gas_bounty, winner_amount),
        );
        
        Ok(())
    }

    /// TASK 2: Activate deficit resolution when clawback is detected
    fn activate_deficit_resolution(env: &Env, round_id: u32, actual_balance: i128) -> Result<(), SusuError> {
        let mut round: Round = env.storage().instance()
            .get(&DataKey::RoundHistory(round_id))
            .ok_or(SusuError::RoundNotFound)?;
        
        let deficit_amount = round.total_pot - actual_balance;
        round.status = RoundStatus::DeficitPaused;
        round.deficit_amount = deficit_amount;
        
        // Calculate recovery surcharge per member
        let surcharge_per_member = (deficit_amount * RECOVERY_SURCHARGE_BPS as i128) 
            / (round.members.len() as i128 * 10000);
        round.recovery_surcharge_per_member = surcharge_per_member;
        
        env.storage().instance().set(&DataKey::RoundHistory(round_id), &round);
        
        // Create deficit vote
        let vote = DeficitVote {
            round_id,
            votes_for_skip: 0,
            votes_against_skip: 0,
            total_voting_power: round.members.len() as u32,
            voting_deadline: env.ledger().timestamp() + VOTING_PERIOD,
            executed: false,
        };
        
        env.storage().instance().set(&DataKey::DeficitVote(round_id), &vote);
        
        env.events().publish(
            (symbol_short!("deficit_detected"),),
            (round_id, deficit_amount, surcharge_per_member),
        );
        
        Err(SusuError::DeficitDetected)
    }

    /// TASK 2: Vote on deficit resolution (skip payout or pay surcharge)
    pub fn vote_on_deficit(env: Env, round_id: u32, vote_for_skip: bool) -> Result<(), SusuError> {
        let voter = env.invoker();
        
        let round: Round = env.storage().instance()
            .get(&DataKey::RoundHistory(round_id))
            .ok_or(SusuError::RoundNotFound)?;
        
        // Verify voter is a member
        let is_member = round.members.iter().any(|m| m == voter);
        if !is_member {
            return Err(SusuError::InvalidMember);
        }
        
        let mut vote: DeficitVote = env.storage().instance()
            .get(&DataKey::DeficitVote(round_id))
            .ok_or(SusuError::VotingPeriodActive)?;
        
        // Check voting period
        let now = env.ledger().timestamp();
        if now > vote.voting_deadline {
            return Err(SusuError::VotingPeriodEnded);
        }
        
        // Record vote
        if vote_for_skip {
            vote.votes_for_skip += 1;
        } else {
            vote.votes_against_skip += 1;
        }
        
        env.storage().instance().set(&DataKey::DeficitVote(round_id), &vote);
        
        env.events().publish(
            (symbol_short!("deficit_vote"),),
            (round_id, voter, vote_for_skip, vote.votes_for_skip, vote.votes_against_skip),
        );
        
        Ok(())
    }

    /// TASK 2: Execute deficit vote result
    pub fn execute_deficit_vote(env: Env, round_id: u32) -> Result<(), SusuError> {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(SusuError::NotInitialized)?;
        admin.require_auth();
        
        let vote: DeficitVote = env.storage().instance()
            .get(&DataKey::DeficitVote(round_id))
            .ok_or(SusuError::VotingPeriodActive)?;
        
        // Check voting period ended
        let now = env.ledger().timestamp();
        if now < vote.voting_deadline {
            return Err(SusuError::VotingPeriodActive);
        }
        
        // Check if threshold met
        let approval_percentage = (vote.votes_for_skip * 10000) / vote.total_voting_power;
        if approval_percentage < DEFICIT_VOTING_THRESHOLD {
            return Err(SusuError::ThresholdNotMet);
        }
        
        // Skip payout - mark round as skipped
        let mut round: Round = env.storage().instance()
            .get(&DataKey::RoundHistory(round_id))
            .ok_or(SusuError::RoundNotFound)?;
        
        round.status = RoundStatus::Skipped;
        round.deficit_vote_passed = true;
        
        env.storage().instance().set(&DataKey::RoundHistory(round_id), &round);
        
        env.events().publish(
            (symbol_short!("deficit_vote_executed"),),
            (round_id, vote.votes_for_skip, vote.total_voting_power),
        );
        
        Ok(())
    }

    /// TASK 3: Get reliability score for inter-contract queries
    pub fn get_reliability_score(env: Env, user: Address) -> Result<u32, SusuError> {
        let member: Member = env.storage().instance()
            .get(&DataKey::Member(user))
            .ok_or(SusuError::InvalidMember)?;
        
        Ok(member.reliability_score)
    }

    /// TASK 3: Verify if user qualifies for grant priority (score > 900)
    pub fn verify_grant_priority_eligible(env: Env, user: Address) -> Result<bool, SusuError> {
        let score = Self::get_reliability_score(env, user)?;
        Ok(score > MIN_RELIABILITY_SCORE)
    }

    /// TASK 3: Set Grant-Stream contract address for interop
    pub fn set_grant_stream_contract(env: Env, admin: Address, contract_addr: Address) -> Result<(), SusuError> {
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(SusuError::NotInitialized)?;
        if admin != stored_admin {
            return Err(SusuError::NotAuthorized);
        }
        
        env.storage().instance().set(&DataKey::GrantStreamContract, &contract_addr);
        
        env.events().publish(
            (symbol_short!("grant_stream_set"),),
            (admin, contract_addr),
        );
        
        Ok(())
    }

    /// Helper: Update member reliability score based on round outcome
    fn update_member_reliability(env: &Env, member: &Address, won_round: bool) -> Result<(), SusuError> {
        let mut member_data: Member = env.storage().instance()
            .get(&DataKey::Member(member))
            .ok_or(SusuError::InvalidMember)?;
        
        member_data.rounds_participated += 1;
        if won_round {
            member_data.rounds_won += 1;
        }
        
        // Calculate new reliability score
        // Formula: (rounds_won / rounds_participated) * 1000
        // Members who win fairly get high scores, those with deficits get penalized
        let base_score = if member_data.rounds_participated > 0 {
            (member_data.rounds_won as u32 * 1000) / member_data.rounds_participated
        } else {
            1000
        };
        
        member_data.reliability_score = base_score.min(1000);
        
        env.storage().instance().set(&DataKey::Member(member), &member_data);
        
        Ok(())
    }

    /// Get round details
    pub fn get_round(env: Env, round_id: u32) -> Result<Round, SusuError> {
        env.storage().instance()
            .get(&DataKey::RoundHistory(round_id))
            .ok_or(SusuError::RoundNotFound)
    }

    /// Get member details
    pub fn get_member(env: Env, member: Address) -> Result<Member, SusuError> {
        env.storage().instance()
            .get(&DataKey::Member(member))
            .ok_or(SusuError::InvalidMember)
    }

    /// Get current round ID
    pub fn get_current_round(env: Env) -> Result<u32, SusuError> {
        env.storage().instance()
            .get(&DataKey::CurrentRound)
            .ok_or(SusuError::RoundNotFound)
    }
}

#[cfg(test)]
mod test;
