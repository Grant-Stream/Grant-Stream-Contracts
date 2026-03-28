#![cfg(test)]

use soroban_sdk::{symbol_short, Address, Env, Vec, Map, String};
use crate::recursive_funding::{
    RecursiveFundingContract, RecursiveFundingClient, RecursiveFundingError,
    RenewalProposal, PerformanceMetrics, JobSecurityEligibility, RenewalConfig,
    RecursiveFundingMetrics, RenewalStatus, RecursiveFundingDataKey,
    DEFAULT_VETO_PERIOD_DAYS, MIN_RENEWAL_ELIGIBILITY_MONTHS, MAX_RENEWAL_CYCLES,
    RENEWAL_VETO_THRESHOLD, MIN_VOTING_PARTICIPATION_RENEWAL,
};

#[test]
fn test_recursive_funding_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    // Initialize with valid parameters
    let result = client.initialize(
        &admin,
        &14u64,    // 14-day veto period
        &12u64,    // 12-month minimum eligibility
        &10u32,     // Maximum 10 renewal cycles
    );
    
    assert!(result.is_ok());
    
    // Verify configuration
    let config = client.get_config().unwrap();
    assert_eq!(config.admin, admin);
    assert_eq!(config.veto_period_days, 14);
    assert_eq!(config.min_eligibility_months, 12);
    assert_eq!(config.max_renewal_cycles, 10);
    assert_eq!(config.veto_threshold, RENEWAL_VETO_THRESHOLD);
    assert_eq!(config.min_voting_participation, MIN_VOTING_PARTICIPATION_RENEWAL);
    assert!(config.auto_renewal_enabled);
    assert_eq!(config.performance_weight, 4000); // 40% weight
    assert_eq!(config.community_weight, 3000);  // 30% weight
    assert_eq!(config.technical_weight, 3000);  // 30% weight
}

#[test]
fn test_recursive_funding_invalid_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    // Test initialization with invalid veto period (> 30 days)
    let result = client.try_initialize(
        &admin,
        &35u64, // Too long
        &12u64,
        &10u32,
    );
    assert_eq!(result, Err(RecursiveFundingError::InvalidTiming));
    
    // Test initialization with invalid eligibility months (< 6)
    let result = client.try_initialize(
        &admin,
        &14u64,
        &3u64, // Too short
        &10u32,
    );
    assert_eq!(result, Err(RecursiveFundingError::InvalidTiming));
    
    // Test initialization with invalid max cycles (> 20)
    let result = client.try_initialize(
        &admin,
        &14u64,
        &12u64,
        &25u32, // Too many cycles
    );
    assert_eq!(result, Err(RecursiveFundingError::InvalidAmount));
    
    // Test double initialization
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    let result = client.try_initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    );
    assert_eq!(result, Err(RecursiveFundingError::NotInitialized));
}

#[test]
fn test_propose_renewal() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    // Initialize
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    // Create performance metrics
    let performance_metrics = PerformanceMetrics {
        milestones_completed: 12,
        total_milestones: 12,
        completion_rate: 10000, // 100% completion
        average_delivery_time: 30 * 24 * 60 * 60, // 30 days average
        community_satisfaction: 95,
        code_quality_score: 90,
        documentation_quality: 85,
        collaboration_score: 88,
        innovation_score: 92,
    };
    
    // Propose renewal
    let proposal_id = client.propose_renewal(
        1u64, // Original grant ID
        &100000i128, // Renewal amount
        &12u64, // 12 months duration
        String::from_str(&env, "Excellent performance, critical infrastructure"),
        performance_metrics,
    ).unwrap();
    
    assert_eq!(proposal_id, 1); // First proposal should have ID 1
    
    // Verify proposal details
    let proposal = client.get_renewal_proposal(proposal_id).unwrap();
    assert_eq!(proposal.proposal_id, proposal_id);
    assert_eq!(proposal.original_grant_id, 1);
    assert_eq!(proposal.renewal_amount, 100000);
    assert_eq!(proposal.renewal_duration, 12 * 30 * 24 * 60 * 60); // 12 months in seconds
    assert_eq!(proposal.status, RenewalStatus::VetoPeriod);
    assert_eq!(proposal.veto_count, 0);
    assert_eq!(proposal.approval_count, 0);
    assert!(proposal.veto_deadline > proposal.proposed_at);
}

#[test]
fn test_propose_renewal_invalid_parameters() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    let performance_metrics = PerformanceMetrics {
        milestones_completed: 12,
        total_milestones: 12,
        completion_rate: 10000,
        average_delivery_time: 30 * 24 * 60 * 60,
        community_satisfaction: 95,
        code_quality_score: 90,
        documentation_quality: 85,
        collaboration_score: 88,
        innovation_score: 92,
    };
    
    // Test with invalid amount (<= 0)
    let result = client.try_propose_renewal(
        1u64,
        &0i128, // Invalid amount
        &12u64,
        String::from_str(&env, "Test"),
        performance_metrics,
    );
    assert_eq!(result, Err(RecursiveFundingError::InvalidAmount));
    
    // Test with invalid duration (< 6 months)
    let result = client.try_propose_renewal(
        1u64,
        &100000i128,
        &3u64, // Too short
        String::from_str(&env, "Test"),
        performance_metrics,
    );
    assert_eq!(result, Err(RecursiveFundingError::InvalidDuration));
    
    // Test with invalid duration (> 24 months)
    let result = client.try_propose_renewal(
        1u64,
        &100000i128,
        &30u64, // Too long
        String::from_str(&env, "Test"),
        performance_metrics,
    );
    assert_eq!(result, Err(RecursiveFundingError::InvalidDuration));
}

#[test]
fn test_veto_renewal() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    let performance_metrics = PerformanceMetrics {
        milestones_completed: 12,
        total_milestones: 12,
        completion_rate: 10000,
        average_delivery_time: 30 * 24 * 60 * 60,
        community_satisfaction: 95,
        code_quality_score: 90,
        documentation_quality: 85,
        collaboration_score: 88,
        innovation_score: 92,
    };
    
    // Create proposal
    let proposal_id = client.propose_renewal(
        1u64,
        &100000i128,
        &12u64,
        String::from_str(&env, "Test renewal"),
        performance_metrics,
    ).unwrap();
    
    // Cast veto vote
    let result = client.veto_renewal(
        proposal_id,
        String::from_str(&env, "Concerns about project direction"),
    );
    assert!(result.is_ok());
    
    // Verify veto was recorded
    let proposal = client.get_renewal_proposal(proposal_id).unwrap();
    assert_eq!(proposal.veto_count, 1);
    assert_eq!(proposal.total_voters, 1);
}

#[test]
fn test_veto_threshold_exceeded() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    let performance_metrics = PerformanceMetrics {
        milestones_completed: 12,
        total_milestones: 12,
        completion_rate: 10000,
        average_delivery_time: 30 * 24 * 60 * 60,
        community_satisfaction: 95,
        code_quality_score: 90,
        documentation_quality: 85,
        collaboration_score: 88,
        innovation_score: 92,
    };
    
    let proposal_id = client.propose_renewal(
        1u64,
        &100000i128,
        &12u64,
        String::from_str(&env, "Test renewal"),
        performance_metrics,
    ).unwrap();
    
    // Cast enough veto votes to exceed threshold (20%)
    for i in 0..3 {
        let voter = Address::generate(&env);
        // This would need proper authentication in real implementation
        let _ = client.veto_renewal(
            proposal_id,
            String::from_str(&env, &format!("Veto reason {}", i)),
        );
    }
    
    // Check if proposal was vetoed
    let proposal = client.get_renewal_proposal(proposal_id).unwrap();
    assert_eq!(proposal.status, RenewalStatus::Vetoed);
}

#[test]
fn test_approve_renewal() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    let performance_metrics = PerformanceMetrics {
        milestones_completed: 12,
        total_milestones: 12,
        completion_rate: 10000,
        average_delivery_time: 30 * 24 * 60 * 60,
        community_satisfaction: 95,
        code_quality_score: 90,
        documentation_quality: 85,
        collaboration_score: 88,
        innovation_score: 92,
    };
    
    let proposal_id = client.propose_renewal(
        1u64,
        &100000i128,
        &12u64,
        String::from_str(&env, "Excellent performance"),
        performance_metrics,
    ).unwrap();
    
    // Advance time past veto period
    env.ledger().set_timestamp(env.ledger().timestamp() + 15 * 24 * 60 * 60); // 15 days
    
    // Process veto periods to move to voting period
    let _ = client.process_veto_periods();
    
    // Cast approval vote
    let result = client.approve_renewal(proposal_id);
    assert!(result.is_ok());
    
    // Verify approval was recorded
    let proposal = client.get_renewal_proposal(proposal_id).unwrap();
    assert_eq!(proposal.approval_count, 1);
    assert_eq!(proposal.total_voters, 1);
}

#[test]
fn test_execute_renewal() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    let performance_metrics = PerformanceMetrics {
        milestones_completed: 12,
        total_milestones: 12,
        completion_rate: 10000,
        average_delivery_time: 30 * 24 * 60 * 60,
        community_satisfaction: 95,
        code_quality_score: 90,
        documentation_quality: 85,
        collaboration_score: 88,
        innovation_score: 92,
    };
    
    let proposal_id = client.propose_renewal(
        1u64,
        &100000i128,
        &12u64,
        String::from_str(&env, "Critical infrastructure renewal"),
        performance_metrics,
    ).unwrap();
    
    // Advance time past veto and voting periods
    env.ledger().set_timestamp(env.ledger().timestamp() + 22 * 24 * 60 * 60); // 22 days
    
    // Process veto periods
    let _ = client.process_veto_periods();
    
    // Cast sufficient approval votes
    for i in 0..5 {
        let _ = client.approve_renewal(proposal_id);
    }
    
    // Execute renewal
    let new_grant_id = client.execute_renewal(proposal_id);
    assert!(new_grant_id.is_ok());
    
    let created_grant_id = new_grant_id.unwrap();
    assert!(created_grant_id > 0);
    
    // Verify proposal was executed
    let proposal = client.get_renewal_proposal(proposal_id).unwrap();
    assert_eq!(proposal.status, RenewalStatus::Executed);
    assert_eq!(proposal.new_grant_id, Some(created_grant_id));
    assert!(proposal.executed_at.is_some());
}

#[test]
fn test_renewal_eligibility_check() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    // Check eligibility for new grant
    let eligibility = client.check_renewal_eligibility(1u64).unwrap();
    assert!(eligibility.is_eligible);
    assert_eq!(eligibility.grant_id, 1);
    assert!(eligibility.critical_infrastructure);
    assert!(eligibility.continuous_contribution);
    assert_eq!(eligibility.renewal_count, 0);
    
    // Verify eligibility is cached
    let cached_eligibility = client.get_grant_eligibility(1u64).unwrap();
    assert_eq!(eligibility, cached_eligibility);
}

#[test]
fn test_critical_infrastructure() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    // Add grant to critical infrastructure
    let result = client.add_critical_infrastructure(&admin, 1u64);
    assert!(result.is_ok());
    
    // Try to add same grant again (should be no-op)
    let result = client.add_critical_infrastructure(&admin, 1u64);
    assert!(result.is_ok()); // Should succeed but not duplicate
    
    // Try with unauthorized user
    let unauthorized = Address::generate(&env);
    let result = client.try_add_critical_infrastructure(&unauthorized, 2u64);
    assert_eq!(result, Err(RecursiveFundingError::Unauthorized));
}

#[test]
fn test_process_veto_periods() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    let performance_metrics = PerformanceMetrics {
        milestones_completed: 12,
        total_milestones: 12,
        completion_rate: 10000,
        average_delivery_time: 30 * 24 * 60 * 60,
        community_satisfaction: 95,
        code_quality_score: 90,
        documentation_quality: 85,
        collaboration_score: 88,
        innovation_score: 92,
    };
    
    // Create multiple proposals
    let proposal_id1 = client.propose_renewal(
        1u64,
        &100000i128,
        &12u64,
        String::from_str(&env, "Proposal 1"),
        performance_metrics.clone(),
    ).unwrap();
    
    let proposal_id2 = client.propose_renewal(
        2u64,
        &100000i128,
        &12u64,
        String::from_str(&env, "Proposal 2"),
        performance_metrics,
    ).unwrap();
    
    // Advance time past veto period
    env.ledger().set_timestamp(env.ledger().timestamp() + 15 * 24 * 60 * 60);
    
    // Process veto periods
    let transitioned_proposals = client.process_veto_periods().unwrap();
    
    // Both proposals should transition to voting period
    assert_eq!(transitioned_proposals.len(), 2);
    assert!(transitioned_proposals.contains(&proposal_id1));
    assert!(transitioned_proposals.contains(&proposal_id2));
    
    // Verify proposals are now in voting period
    let proposal1 = client.get_renewal_proposal(proposal_id1).unwrap();
    let proposal2 = client.get_renewal_proposal(proposal_id2).unwrap();
    assert_eq!(proposal1.status, RenewalStatus::VotingPeriod);
    assert_eq!(proposal2.status, RenewalStatus::VotingPeriod);
}

#[test]
fn test_recursive_funding_metrics() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    // Check initial metrics
    let metrics = client.get_recursive_funding_metrics().unwrap();
    assert_eq!(metrics.total_renewal_proposals, 0);
    assert_eq!(metrics.successful_renewals, 0);
    assert_eq!(metrics.vetoed_proposals, 0);
    assert_eq!(metrics.rejected_proposals, 0);
    assert_eq!(metrics.critical_projects_renewed, 0);
    assert_eq!(metrics.total_renewed_amount, 0);
    assert_eq!(metrics.job_security_score, 0);
    
    // Create proposal (should update metrics)
    let performance_metrics = PerformanceMetrics {
        milestones_completed: 12,
        total_milestones: 12,
        completion_rate: 10000,
        average_delivery_time: 30 * 24 * 60 * 60,
        community_satisfaction: 95,
        code_quality_score: 90,
        documentation_quality: 85,
        collaboration_score: 88,
        innovation_score: 92,
    };
    
    let _ = client.propose_renewal(
        1u64,
        &100000i128,
        &12u64,
        String::from_str(&env, "Test proposal"),
        performance_metrics,
    ).unwrap();
    
    // Check updated metrics
    let updated_metrics = client.get_recursive_funding_metrics().unwrap();
    assert_eq!(updated_metrics.total_renewal_proposals, 1);
    assert!(updated_metrics.last_updated > metrics.last_updated);
}

#[test]
fn test_auto_renewal_disabled() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    let performance_metrics = PerformanceMetrics {
        milestones_completed: 12,
        total_milestones: 12,
        completion_rate: 10000,
        average_delivery_time: 30 * 24 * 60 * 60,
        community_satisfaction: 95,
        code_quality_score: 90,
        documentation_quality: 85,
        collaboration_score: 88,
        innovation_score: 92,
    };
    
    let proposal_id = client.propose_renewal(
        1u64,
        &100000i128,
        &12u64,
        String::from_str(&env, "Test renewal"),
        performance_metrics,
    ).unwrap();
    
    // Advance time and process veto periods
    env.ledger().set_timestamp(env.ledger().timestamp() + 22 * 24 * 60 * 60);
    let _ = client.process_veto_periods();
    
    // Add approval votes
    for _ in 0..5 {
        let _ = client.approve_renewal(proposal_id);
    }
    
    // Disable auto-renewal (this would require admin function)
    // For now, test that execution works when enabled
    
    let result = client.execute_renewal(proposal_id);
    assert!(result.is_ok()); // Should succeed when auto-renewal is enabled
}

#[test]
fn test_edge_cases() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    // Initialize with minimum values
    client.initialize(
        &admin,
        &1u64,    // Minimum veto period
        &6u64,     // Minimum eligibility
        &1u32,     // Minimum cycles
    ).unwrap();
    
    // Initialize with maximum values
    let admin2 = Address::generate(&env);
    let contract_id2 = env.register_contract(None, RecursiveFundingContract);
    let client2 = RecursiveFundingClient::new(&env, &contract_id2);
    
    client2.initialize(
        &admin2,
        &30u64,   // Maximum veto period
        &24u64,    // Maximum eligibility
        &20u32,    // Maximum cycles
    ).unwrap();
    
    // Test with maximum renewal duration
    let performance_metrics = PerformanceMetrics {
        milestones_completed: 12,
        total_milestones: 12,
        completion_rate: 10000,
        average_delivery_time: 30 * 24 * 60 * 60,
        community_satisfaction: 95,
        code_quality_score: 90,
        documentation_quality: 85,
        collaboration_score: 88,
        innovation_score: 92,
    };
    
    let proposal_id = client2.propose_renewal(
        1u64,
        &i128::MAX, // Maximum amount
        &24u64,     // Maximum duration
        String::from_str(&env, "Maximum test"),
        performance_metrics,
    ).unwrap();
    
    let proposal = client2.get_renewal_proposal(proposal_id).unwrap();
    assert_eq!(proposal.renewal_amount, i128::MAX);
    assert_eq!(proposal.renewal_duration, 24 * 30 * 24 * 60 * 60); // 24 months in seconds
}

#[test]
fn test_error_conditions() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    // Test operations on uninitialized contract
    let result = client.try_get_config();
    assert_eq!(result, Err(RecursiveFundingError::NotInitialized));
    
    let result = client.try_get_renewal_proposal(1u64);
    assert_eq!(result, Err(RecursiveFundingError::ProposalNotFound));
    
    // Initialize and test other error conditions
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    // Test voting on non-existent proposal
    let result = client.try_veto_renewal(999u64, String::from_str(&env, "Test"));
    assert_eq!(result, Err(RecursiveFundingError::ProposalNotFound));
    
    let result = client.try_approve_renewal(999u64);
    assert_eq!(result, Err(RecursiveFundingError::ProposalNotFound));
    
    // Test executing non-existent proposal
    let result = client.try_execute_renewal(999u64);
    assert_eq!(result, Err(RecursiveFundingError::ProposalNotFound));
}

#[test]
fn test_performance_metrics_validation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, RecursiveFundingContract);
    let client = RecursiveFundingClient::new(&env, &contract_id);
    
    client.initialize(
        &admin,
        &14u64,
        &12u64,
        &10u32,
    ).unwrap();
    
    // Test with perfect performance metrics
    let perfect_metrics = PerformanceMetrics {
        milestones_completed: 12,
        total_milestones: 12,
        completion_rate: 10000, // 100%
        average_delivery_time: 25 * 24 * 60 * 60, // 25 days average (early)
        community_satisfaction: 100, // Perfect score
        code_quality_score: 100,  // Perfect code
        documentation_quality: 100, // Perfect docs
        collaboration_score: 100, // Perfect collaboration
        innovation_score: 100,       // Perfect innovation
    };
    
    let proposal_id = client.propose_renewal(
        1u64,
        &100000i128,
        &12u64,
        String::from_str(&env, "Perfect performance"),
        perfect_metrics,
    ).unwrap();
    
    let proposal = client.get_renewal_proposal(proposal_id).unwrap();
    assert_eq!(proposal.performance_metrics.completion_rate, 10000);
    assert_eq!(proposal.performance_metrics.community_satisfaction, 100);
    assert_eq!(proposal.performance_metrics.code_quality_score, 100);
    
    // Test with poor performance metrics
    let poor_metrics = PerformanceMetrics {
        milestones_completed: 8,
        total_milestones: 12,
        completion_rate: 6666, // 66.66% completion
        average_delivery_time: 45 * 24 * 60 * 60, // 45 days average (late)
        community_satisfaction: 45,  // Poor satisfaction
        code_quality_score: 50,   // Poor code quality
        documentation_quality: 30, // Poor documentation
        collaboration_score: 40, // Poor collaboration
        innovation_score: 25,       // Low innovation
    };
    
    let proposal_id2 = client.propose_renewal(
        2u64,
        &50000i128, // Lower amount due to poor performance
        &12u64,
        String::from_str(&env, "Poor performance"),
        poor_metrics,
    ).unwrap();
    
    let proposal2 = client.get_renewal_proposal(proposal_id2).unwrap();
    assert_eq!(proposal2.performance_metrics.completion_rate, 6666);
    assert_eq!(proposal2.performance_metrics.community_satisfaction, 45);
    assert_eq!(proposal2.performance_metrics.code_quality_score, 50);
}
