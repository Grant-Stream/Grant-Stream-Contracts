#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, u64,
};

// --- Constants ---
const MIN_TEMPORAL_SEPARATION: u64 = 1; // Minimum 1 ledger between operations
const FLASH_LOAN_DETECTION_WINDOW: u64 = 5; // 5 seconds window for flash loan detection

// --- Types ---

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum InteractionType {
    Vote,
    Withdraw,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct InteractionRecord {
    pub interaction_type: InteractionType,
    pub ledger: u64,
    pub timestamp: u64,
    pub grant_id: Option<u64>, // Optional grant_id for withdrawal operations
    pub proposal_id: Option<u64>, // Optional proposal_id for voting operations
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracterror]
#[repr(u32)]
pub enum TemporalGuardError {
    NotInitialized = 101,
    Unauthorized = 102,
    SameLedgerInteraction = 103,
    FlashLoanDetected = 104,
    InvalidAddress = 105,
    InvalidGrantId = 106,
    InvalidProposalId = 107,
    InteractionNotFound = 108,
    MathOverflow = 109,
}

#[derive(Clone)]
#[contracttype]
pub enum TemporalGuardDataKey {
    Admin,
    LastInteraction(Address), // Maps address to their last interaction
    InteractionHistory(Address, u64), // Maps address + nonce to interaction details
    Nonce(Address), // Maps address to their interaction nonce
    FlashLoanFlag(Address), // Temporary flag for flash loan detection
    TemporalSeparationRequired, // Configuration: minimum ledgers between operations
}

/// Temporal Guard Contract - Flash Loan Protection Wrapper
/// 
/// This contract implements a "Speed Bump" that prevents any address from voting 
/// and withdrawing from a grant pool within the same ledger. This protects the 
/// DAO from sophisticated atomic exploits where malicious actors could borrow 
/// millions in XLM, vote to approve their own grant, and withdraw funds all 
/// in a single 5-second window.
pub struct TemporalGuardContract;

#[contractimpl]
impl TemporalGuardContract {
    /// Initialize the Temporal Guard contract
    /// 
    /// # Arguments
    /// * `admin` - The admin address that can configure the contract
    /// * `temporal_separation` - Minimum number of ledgers required between operations (default: 1)
    pub fn initialize(
        env: Env,
        admin: Address,
        temporal_separation: Option<u64>,
    ) -> Result<(), TemporalGuardError> {
        if env.storage().instance().has(&TemporalGuardDataKey::Admin) {
            return Err(TemporalGuardError::NotInitialized);
        }

        admin.require_auth();

        env.storage().instance().set(&TemporalGuardDataKey::Admin, &admin);
        env.storage().instance().set(
            &TemporalGuardDataKey::TemporalSeparationRequired,
            &temporal_separation.unwrap_or(MIN_TEMPORAL_SEPARATION),
        );

        env.events().publish(
            (symbol_short!("temp_guard_init"),),
            (admin, temporal_separation.unwrap_or(MIN_TEMPORAL_SEPARATION)),
        );

        Ok(())
    }

    /// Check if an address can perform a specific interaction type
    /// This is the main protection function that prevents same-ledger operations
    /// 
    /// # Arguments
    /// * `user` - The address attempting the interaction
    /// * `interaction_type` - The type of interaction (Vote or Withdraw)
    /// * `grant_id` - Optional grant_id for withdrawal operations
    /// * `proposal_id` - Optional proposal_id for voting operations
    /// 
    /// # Returns
    /// * `Result<(), TemporalGuardError>` - Ok if allowed, Err if blocked
    pub fn check_interaction_allowed(
        env: Env,
        user: Address,
        interaction_type: InteractionType,
        grant_id: Option<u64>,
        proposal_id: Option<u64>,
    ) -> Result<(), TemporalGuardError> {
        let current_ledger = env.ledger().sequence;
        let current_timestamp = env.ledger().timestamp();

        // Check for flash loan patterns
        Self::detect_flash_loan_attempt(&env, &user, current_ledger, current_timestamp)?;

        // Get the user's last interaction
        let last_interaction: Option<InteractionRecord> = env
            .storage()
            .instance()
            .get(&TemporalGuardDataKey::LastInteraction(user.clone()));

        if let Some(last) = last_interaction {
            let required_separation = Self::get_temporal_separation(&env)?;
            
            // Check if same ledger interaction is attempted
            if current_ledger == last.ledger {
                // Allow multiple interactions of the same type in the same ledger
                // but block cross-type interactions (vote -> withdraw or withdraw -> vote)
                if last.interaction_type != interaction_type {
                    return Err(TemporalGuardError::SameLedgerInteraction);
                }
            }
            
            // Check temporal separation requirement
            if current_ledger < last.ledger.checked_add(required_separation).ok_or(TemporalGuardError::MathOverflow)? {
                return Err(TemporalGuardError::SameLedgerInteraction);
            }

            // Additional check: prevent voting immediately after withdrawal or vice versa
            // even across different ledgers if they're too close in time (flash loan pattern)
            if matches!(interaction_type, InteractionType::Vote) && matches!(last.interaction_type, InteractionType::Withdraw) {
                if current_timestamp < last.timestamp.checked_add(FLASH_LOAN_DETECTION_WINDOW).ok_or(TemporalGuardError::MathOverflow)? {
                    return Err(TemporalGuardError::FlashLoanDetected);
                }
            }
            if matches!(interaction_type, InteractionType::Withdraw) && matches!(last.interaction_type, InteractionType::Vote) {
                if current_timestamp < last.timestamp.checked_add(FLASH_LOAN_DETECTION_WINDOW).ok_or(TemporalGuardError::MathOverflow)? {
                    return Err(TemporalGuardError::FlashLoanDetected);
                }
            }
        }

        Ok(())
    }

    /// Record an interaction after it has been successfully completed
    /// This should be called AFTER the actual operation succeeds
    /// 
    /// # Arguments
    /// * `user` - The address that performed the interaction
    /// * `interaction_type` - The type of interaction performed
    /// * `grant_id` - Optional grant_id for withdrawal operations
    /// * `proposal_id` - Optional proposal_id for voting operations
    pub fn record_interaction(
        env: Env,
        user: Address,
        interaction_type: InteractionType,
        grant_id: Option<u64>,
        proposal_id: Option<u64>,
    ) -> Result<(), TemporalGuardError> {
        let current_ledger = env.ledger().sequence;
        let current_timestamp = env.ledger().timestamp();

        let interaction = InteractionRecord {
            interaction_type,
            ledger: current_ledger,
            timestamp: current_timestamp,
            grant_id,
            proposal_id,
        };

        // Update last interaction
        env.storage()
            .instance()
            .set(&TemporalGuardDataKey::LastInteraction(user.clone()), &interaction);

        // Update interaction history
        let nonce = Self::get_and_increment_nonce(&env, &user)?;
        env.storage()
            .instance()
            .set(&TemporalGuardDataKey::InteractionHistory(user.clone(), nonce), &interaction);

        // Clear any flash loan flags
        env.storage()
            .instance()
            .remove(&TemporalGuardDataKey::FlashLoanFlag(user));

        env.events().publish(
            (symbol_short!("interaction_recorded"),),
            (user, interaction_type, current_ledger, current_timestamp),
        );

        Ok(())
    }

    /// Wrapper function for voting operations
    /// This should be called before any voting operation
    /// 
    /// # Arguments
    /// * `voter` - The address attempting to vote
    /// * `proposal_id` - The proposal ID being voted on
    pub fn check_vote_allowed(
        env: Env,
        voter: Address,
        proposal_id: u64,
    ) -> Result<(), TemporalGuardError> {
        Self::check_interaction_allowed(
            env,
            voter,
            InteractionType::Vote,
            None,
            Some(proposal_id),
        )
    }

    /// Wrapper function for withdrawal operations
    /// This should be called before any withdrawal operation
    /// 
    /// # Arguments
    /// * `recipient` - The address attempting to withdraw
    /// * `grant_id` - The grant ID being withdrawn from
    pub fn check_withdraw_allowed(
        env: Env,
        recipient: Address,
        grant_id: u64,
    ) -> Result<(), TemporalGuardError> {
        Self::check_interaction_allowed(
            env,
            recipient,
            InteractionType::Withdraw,
            Some(grant_id),
            None,
        )
    }

    /// Record a successful vote
    /// This should be called after a vote is successfully cast
    /// 
    /// # Arguments
    /// * `voter` - The address that voted
    /// * `proposal_id` - The proposal ID that was voted on
    pub fn record_vote(
        env: Env,
        voter: Address,
        proposal_id: u64,
    ) -> Result<(), TemporalGuardError> {
        Self::record_interaction(
            env,
            voter,
            InteractionType::Vote,
            None,
            Some(proposal_id),
        )
    }

    /// Record a successful withdrawal
    /// This should be called after a withdrawal is successfully completed
    /// 
    /// # Arguments
    /// * `recipient` - The address that withdrew
    /// * `grant_id` - The grant ID that was withdrawn from
    pub fn record_withdrawal(
        env: Env,
        recipient: Address,
        grant_id: u64,
    ) -> Result<(), TemporalGuardError> {
        Self::record_interaction(
            env,
            recipient,
            InteractionType::Withdraw,
            Some(grant_id),
            None,
        )
    }

    /// Admin function to update temporal separation requirements
    /// 
    /// # Arguments
    /// * `admin` - The admin address
    /// * `new_separation` - New minimum separation in ledgers
    pub fn update_temporal_separation(
        env: Env,
        admin: Address,
        new_separation: u64,
    ) -> Result<(), TemporalGuardError> {
        Self::require_admin_auth(&env, &admin)?;
        
        if new_separation == 0 {
            return Err(TemporalGuardError::InvalidAddress); // Reuse error for invalid parameter
        }

        env.storage()
            .instance()
            .set(&TemporalGuardDataKey::TemporalSeparationRequired, &new_separation);

        env.events().publish(
            (symbol_short!("temp_sep_updated"),),
            (admin, new_separation),
        );

        Ok(())
    }

    /// Get the last interaction for a user
    /// 
    /// # Arguments
    /// * `user` - The address to query
    /// 
    /// # Returns
    /// * `Result<Option<InteractionRecord>, TemporalGuardError>` - The last interaction or None
    pub fn get_last_interaction(
        env: Env,
        user: Address,
    ) -> Result<Option<InteractionRecord>, TemporalGuardError> {
        Ok(env
            .storage()
            .instance()
            .get(&TemporalGuardDataKey::LastInteraction(user)))
    }

    /// Check if an address is currently flagged for potential flash loan abuse
    /// 
    /// # Arguments
    /// * `user` - The address to check
    /// 
    /// # Returns
    /// * `Result<bool, TemporalGuardError>` - True if flagged, false otherwise
    pub fn is_flash_loan_suspect(
        env: Env,
        user: Address,
    ) -> Result<bool, TemporalGuardError> {
        Ok(env
            .storage()
            .instance()
            .has(&TemporalGuardDataKey::FlashLoanFlag(user)))
    }

    // --- Helper Functions ---

    fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), TemporalGuardError> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&TemporalGuardDataKey::Admin)
            .ok_or(TemporalGuardError::NotInitialized)?;
        
        if *admin != stored_admin {
            return Err(TemporalGuardError::Unauthorized);
        }
        
        admin.require_auth();
        Ok(())
    }

    fn get_temporal_separation(env: &Env) -> Result<u64, TemporalGuardError> {
        env.storage()
            .instance()
            .get(&TemporalGuardDataKey::TemporalSeparationRequired)
            .ok_or(TemporalGuardError::NotInitialized)
    }

    fn get_and_increment_nonce(env: &Env, user: &Address) -> Result<u64, TemporalGuardError> {
        let current_nonce: u64 = env
            .storage()
            .instance()
            .get(&TemporalGuardDataKey::Nonce(user.clone()))
            .unwrap_or(0);
        
        let next_nonce = current_nonce.checked_add(1).ok_or(TemporalGuardError::MathOverflow)?;
        env.storage()
            .instance()
            .set(&TemporalGuardDataKey::Nonce(user.clone()), &next_nonce);
        
        Ok(current_nonce)
    }

    fn detect_flash_loan_attempt(
        env: &Env,
        user: &Address,
        current_ledger: u64,
        current_timestamp: u64,
    ) -> Result<(), TemporalGuardError> {
        // Check if user has a flash loan flag
        if env.storage().instance().has(&TemporalGuardDataKey::FlashLoanFlag(user.clone())) {
            return Err(TemporalGuardError::FlashLoanDetected);
        }

        // Check for suspicious patterns: multiple interactions in very short time
        let last_interaction: Option<InteractionRecord> = env
            .storage()
            .instance()
            .get(&TemporalGuardDataKey::LastInteraction(user.clone()));

        if let Some(last) = last_interaction {
            // If multiple interactions in the same 5-second window, flag as potential flash loan
            if current_timestamp < last.timestamp.checked_add(FLASH_LOAN_DETECTION_WINDOW).ok_or(TemporalGuardError::MathOverflow)? {
                if current_ledger != last.ledger {
                    // Different ledgers but very close in time - suspicious
                    env.storage()
                        .instance()
                        .set(&TemporalGuardDataKey::FlashLoanFlag(user.clone()), &current_timestamp);
                    return Err(TemporalGuardError::FlashLoanDetected);
                }
            }
        }

        Ok(())
    }
}
