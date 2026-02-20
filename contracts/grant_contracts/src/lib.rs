#![no_std]
use soroban_sdk::{
    contract, contractimpl, map, symbol_short, vec, Address, Bytes, Env, Map, String, Symbol, Vec, Val,
};

// Contract for managing milestone-based grant unlocking
// Grants can be unlocked via admin approval of specific milestones
// Includes DAO governance for pausing grants via council voting

const VOTE_THRESHOLD: u32 = 3; // 3-of-5 votes required to pause
const COUNCIL_SIZE: u32 = 5;

#[derive(Clone)]
pub struct Grant {
    pub admin: Address,
    pub grantee: Address,
    pub total_amount: i128,
    pub released_amount: i128,
    pub is_paused: bool,
}

#[derive(Clone)]
pub struct Milestone {
    pub amount: i128,
    pub status: u32, // 0 = Pending, 1 = Approved, 2 = Released
    pub description: String,
}

#[derive(Clone)]
pub struct PauseProposal {
    pub grant_id: Symbol,
    pub proposer: Address,
    pub vote_count: u32,
    pub executed: bool,
    pub voters: Vec<Address>, // Track who has voted
}

#[contract]
pub struct GrantContract;

#[contractimpl]
impl GrantContract {
    /// Create a new grant with an admin and grantee
    /// Only called once per grant ID
    ///
    /// Args:
    /// - grant_id: Unique identifier for the grant
    /// - admin: Admin address who can approve milestones
    /// - grantee: Address receiving the grant funds
    /// - total_amount: Total grant amount in stroops
    pub fn create_grant(
        env: Env,
        grant_id: Symbol,
        admin: Address,
        grantee: Address,
        total_amount: i128,
    ) -> Result<Symbol, String> {
        // Verify admin address
        admin.require_auth();

        // Check if grant already exists
        if env.storage().persistent().has(&grant_id) {
            return Err(String::from_str(&env, "Grant already exists"));
        }

        // Create and store grant
        let grant = Grant {
            admin: admin.clone(),
            grantee: grantee.clone(),
            total_amount,
            released_amount: 0,
            is_paused: false,
        };

        env.storage()
            .persistent()
            .set(&grant_id, &grant);

        env.events()
            .publish((symbol_short!("grant"), symbol_short!("created")), grant_id.clone());

        Ok(grant_id)
    }

    /// Add a new milestone to a grant
    /// Only the admin can call this
    ///
    /// Args:
    /// - grant_id: ID of the grant
    /// - milestone_id: Unique identifier for the milestone
    /// - amount: Amount to be released when milestone is approved
    /// - description: Description of the milestone
    pub fn add_milestone(
        env: Env,
        grant_id: Symbol,
        milestone_id: Symbol,
        amount: i128,
        description: String,
    ) -> Result<Symbol, String> {
        // Get grant to verify admin
        let grant: Grant = env
            .storage()
            .persistent()
            .get(&grant_id)
            .ok_or(String::from_str(&env, "Grant not found"))?;

        grant.admin.require_auth();

        // Create milestone key
        let mut milestone_key_string = String::from_str(&env, "milestone:");
        milestone_key_string.append(&grant_id.to_string());
        milestone_key_string.append(&String::from_str(&env, ":"));
        milestone_key_string.append(&milestone_id.to_string());
        
        let milestone_key = Symbol::new(&env, &milestone_key_string);

        // Check if milestone already exists
        if env.storage().persistent().has(&milestone_key) {
            return Err(String::from_str(&env, "Milestone already exists"));
        }

        // Create and store milestone
        let milestone = Milestone {
            amount,
            status: 0, // Pending
            description,
        };

        env.storage()
            .persistent()
            .set(&milestone_key, &milestone);

        env.events().publish(
            (symbol_short!("milestone"), symbol_short!("created")),
            (grant_id.clone(), milestone_id.clone()),
        );

        Ok(milestone_id)
    }

    /// Get milestone details
    pub fn get_milestone(
        env: Env,
        grant_id: Symbol,
        milestone_id: Symbol,
    ) -> Result<(i128, u32, String), String> {
        // Create milestone key
        let mut milestone_key_string = String::from_str(&env, "milestone:");
        milestone_key_string.append(&grant_id.to_string());
        milestone_key_string.append(&String::from_str(&env, ":"));
        milestone_key_string.append(&milestone_id.to_string());
        
        let milestone_key = Symbol::new(&env, &milestone_key_string);

        let milestone: Milestone = env
            .storage()
            .persistent()
            .get(&milestone_key)
            .ok_or(String::from_str(&env, "Milestone not found"))?;

        Ok((milestone.amount, milestone.status, milestone.description))
    }

    /// Approve a milestone and release funds immediately to grantee
    /// Only admin can call this
    /// Grant must not be paused
    ///
    /// Args:
    /// - grant_id: ID of the grant
    /// - milestone_id: ID of the milestone to approve
    pub fn approve_milestone(
        env: Env,
        grant_id: Symbol,
        milestone_id: Symbol,
    ) -> Result<i128, String> {
        // Get grant
        let mut grant: Grant = env
            .storage()
            .persistent()
            .get(&grant_id)
            .ok_or(String::from_str(&env, "Grant not found"))?;

        grant.admin.require_auth();

        // Check if grant is paused
        if grant.is_paused {
            return Err(String::from_str(&env, "Grant is paused"));
        }

        // Get milestone
        let mut milestone_key_string = String::from_str(&env, "milestone:");
        milestone_key_string.append(&grant_id.to_string());
        milestone_key_string.append(&String::from_str(&env, ":"));
        milestone_key_string.append(&milestone_id.to_string());
        
        let milestone_key = Symbol::new(&env, &milestone_key_string);

        let mut milestone: Milestone = env
            .storage()
            .persistent()
            .get(&milestone_key)
            .ok_or(String::from_str(&env, "Milestone not found"))?;

        // Check if already released
        if milestone.status == 2 {
            return Err(String::from_str(&env, "Milestone already released"));
        }

        // Check if total released + this amount exceeds total grant
        if grant.released_amount + milestone.amount > grant.total_amount {
            return Err(String::from_str(&env, "Exceeds total grant amount"));
        }

        // Update milestone status to Released
        milestone.status = 2;
        env.storage()
            .persistent()
            .set(&milestone_key, &milestone);

        // Update grant released amount
        grant.released_amount += milestone.amount;
        env.storage()
            .persistent()
            .set(&grant_id, &grant);

        // Emit event with released amount
        env.events().publish(
            (symbol_short!("milestone"), symbol_short!("released")),
            (grant_id.clone(), milestone_id.clone(), milestone.amount),
        );

        Ok(milestone.amount)
    }

    /// Get grant details
    pub fn get_grant(
        env: Env,
        grant_id: Symbol,
    ) -> Result<(Address, Address, i128, i128), String> {
        let grant: Grant = env
            .storage()
            .persistent()
            .get(&grant_id)
            .ok_or(String::from_str(&env, "Grant not found"))?;

        Ok((
            grant.admin,
            grant.grantee,
            grant.total_amount,
            grant.released_amount,
        ))
    }

    /// Get total released amount for a grant
    pub fn get_released_amount(env: Env, grant_id: Symbol) -> Result<i128, String> {
        let grant: Grant = env
            .storage()
            .persistent()
            .get(&grant_id)
            .ok_or(String::from_str(&env, "Grant not found"))?;

        Ok(grant.released_amount)
    }

    /// Get remaining amount available in a grant
    pub fn get_remaining_amount(env: Env, grant_id: Symbol) -> Result<i128, String> {
        let grant: Grant = env
            .storage()
            .persistent()
            .get(&grant_id)
            .ok_or(String::from_str(&env, "Grant not found"))?;

        Ok(grant.total_amount - grant.released_amount)
    }

    /// Set council members for DAO governance (5 members required)
    /// Only the grant admin can call this, typically once during setup
    ///
    /// Args:
    /// - grant_id: ID of the grant
    /// - council_members: Vector of 5 council member addresses
    pub fn set_council_members(
        env: Env,
        grant_id: Symbol,
        council_members: Vec<Address>,
    ) -> Result<(), String> {
        // Get grant to verify admin
        let grant: Grant = env
            .storage()
            .persistent()
            .get(&grant_id)
            .ok_or(String::from_str(&env, "Grant not found"))?;

        grant.admin.require_auth();

        // Validate council size
        if council_members.len() != COUNCIL_SIZE as usize {
            return Err(String::from_str(
                &env,
                "Council must have exactly 5 members",
            ));
        }

        // Create council key
        let mut council_key_string = String::from_str(&env, "council:");
        council_key_string.append(&grant_id.to_string());
        let council_key = Symbol::new(&env, &council_key_string);

        // Store council members
        env.storage()
            .persistent()
            .set(&council_key, &council_members);

        env.events().publish(
            (symbol_short!("council"), symbol_short!("set")),
            (grant_id.clone(), council_members.len()),
        );

        Ok(())
    }

    /// Get council members for a grant
    pub fn get_council_members(
        env: Env,
        grant_id: Symbol,
    ) -> Result<Vec<Address>, String> {
        let mut council_key_string = String::from_str(&env, "council:");
        council_key_string.append(&grant_id.to_string());
        let council_key = Symbol::new(&env, &council_key_string);

        env.storage()
            .persistent()
            .get(&council_key)
            .ok_or(String::from_str(&env, "Council not found"))
    }

    /// Propose to pause the grant stream
    /// Any council member can propose a pause
    ///
    /// Args:
    /// - grant_id: ID of the grant to pause
    pub fn propose_pause(env: Env, grant_id: Symbol) -> Result<(), String> {
        // Verify grant exists
        let _grant: Grant = env
            .storage()
            .persistent()
            .get(&grant_id)
            .ok_or(String::from_str(&env, "Grant not found"))?;

        // Get council
        let mut council_key_string = String::from_str(&env, "council:");
        council_key_string.append(&grant_id.to_string());
        let council_key = Symbol::new(&env, &council_key_string);

        let council: Vec<Address> = env
            .storage()
            .persistent()
            .get(&council_key)
            .ok_or(String::from_str(&env, "Council not found"))?;

        // Verify proposer is a council member (but don't require auth from all)
        let proposer = env.invoker();

        let mut is_council_member = false;
        for i in 0..council.len() {
            if council.get_unchecked(i) == proposer {
                is_council_member = true;
                break;
            }
        }

        if !is_council_member {
            return Err(String::from_str(&env, "Only council members can propose"));
        }

        // Create or get pause proposal
        let mut proposal_key_string = String::from_str(&env, "proposal:");
        proposal_key_string.append(&grant_id.to_string());
        let proposal_key = Symbol::new(&env, &proposal_key_string);

        // Check if proposal already exists
        if env.storage().persistent().has(&proposal_key) {
            return Err(String::from_str(&env, "Proposal already exists"));
        }

        // Create new proposal
        let proposal = PauseProposal {
            grant_id: grant_id.clone(),
            proposer: proposer.clone(),
            vote_count: 0,
            executed: false,
            voters: vec![&env],
        };

        env.storage()
            .persistent()
            .set(&proposal_key, &proposal);

        env.events().publish(
            (symbol_short!("proposal"), symbol_short!("created")),
            grant_id.clone(),
        );

        Ok(())
    }

    /// Vote on a pause proposal
    /// Only council members can vote
    ///
    /// Args:
    /// - grant_id: ID of the grant
    pub fn vote(env: Env, grant_id: Symbol) -> Result<bool, String> {
        // Get council
        let mut council_key_string = String::from_str(&env, "council:");
        council_key_string.append(&grant_id.to_string());
        let council_key = Symbol::new(&env, &council_key_string);

        let council: Vec<Address> = env
            .storage()
            .persistent()
            .get(&council_key)
            .ok_or(String::from_str(&env, "Council not found"))?;

        // Get voter
        let voter = env.invoker();

        // Verify voter is a council member
        let mut is_council_member = false;
        for i in 0..council.len() {
            if council.get_unchecked(i) == voter {
                is_council_member = true;
                break;
            }
        }

        if !is_council_member {
            return Err(String::from_str(&env, "Only council members can vote"));
        }

        // Get proposal
        let mut proposal_key_string = String::from_str(&env, "proposal:");
        proposal_key_string.append(&grant_id.to_string());
        let proposal_key = Symbol::new(&env, &proposal_key_string);

        let mut proposal: PauseProposal = env
            .storage()
            .persistent()
            .get(&proposal_key)
            .ok_or(String::from_str(&env, "Proposal not found"))?;

        // Check if already executed
        if proposal.executed {
            return Err(String::from_str(&env, "Proposal already executed"));
        }

        // Check if voter already voted
        for i in 0..proposal.voters.len() {
            if proposal.voters.get_unchecked(i) == voter {
                return Err(String::from_str(&env, "Already voted"));
            }
        }

        // Add vote
        proposal.voters.push_back(voter.clone());
        proposal.vote_count += 1;

        // Check if threshold met
        let mut should_execute = false;
        if proposal.vote_count >= VOTE_THRESHOLD {
            should_execute = true;
            proposal.executed = true;
        }

        // Update proposal
        env.storage()
            .persistent()
            .set(&proposal_key, &proposal);

        // If threshold met, pause the grant
        if should_execute {
            let mut grant: Grant = env
                .storage()
                .persistent()
                .get(&grant_id)
                .ok_or(String::from_str(&env, "Grant not found"))?;

            grant.is_paused = true;

            env.storage()
                .persistent()
                .set(&grant_id, &grant);

            env.events().publish(
                (symbol_short!("grant"), symbol_short!("paused")),
                (grant_id.clone(), proposal.vote_count),
            );
        }

        env.events().publish(
            (symbol_short!("vote"), symbol_short!("cast")),
            (grant_id.clone(), proposal.vote_count),
        );

        Ok(should_execute)
    }

    /// Get pause proposal details
    pub fn get_pause_proposal(
        env: Env,
        grant_id: Symbol,
    ) -> Result<(Address, u32, bool, u32), String> {
        let mut proposal_key_string = String::from_str(&env, "proposal:");
        proposal_key_string.append(&grant_id.to_string());
        let proposal_key = Symbol::new(&env, &proposal_key_string);

        let proposal: PauseProposal = env
            .storage()
            .persistent()
            .get(&proposal_key)
            .ok_or(String::from_str(&env, "Proposal not found"))?;

        Ok((
            proposal.proposer,
            proposal.vote_count,
            proposal.executed,
            VOTE_THRESHOLD,
        ))
    }

    /// Check if a grant is paused
    pub fn is_paused(env: Env, grant_id: Symbol) -> Result<bool, String> {
        let grant: Grant = env
            .storage()
            .persistent()
            .get(&grant_id)
            .ok_or(String::from_str(&env, "Grant not found"))?;

        Ok(grant.is_paused)
    }

    /// Get grant details including pause status
    pub fn get_grant_full(
        env: Env,
        grant_id: Symbol,
    ) -> Result<(Address, Address, i128, i128, bool), String> {
        let grant: Grant = env
            .storage()
            .persistent()
            .get(&grant_id)
            .ok_or(String::from_str(&env, "Grant not found"))?;

        Ok((
            grant.admin,
            grant.grantee,
            grant.total_amount,
            grant.released_amount,
            grant.is_paused,
        ))
    }
}

mod test;
