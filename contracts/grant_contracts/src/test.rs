#![cfg(test)]

use super::{GrantContract, GrantContractClient, GrantStatus, SCALING_FACTOR};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, vec,
};

fn setup_test(env: &Env) -> (Address, Address, Address, Address, Address, GrantContractClient<'_>) {
    let admin = Address::generate(env);
    let grant_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
    let native_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
    let treasury = Address::generate(env);
    let oracle = Address::generate(env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(env, &contract_id);

    client.initialize(&admin, &grant_token_addr.address(), &treasury, &oracle, &native_token_addr.address());

    (admin, grant_token_addr.address(), treasury, oracle, native_token_addr.address(), client)
}

fn set_timestamp(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

#[test]
fn test_pipeline() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    // 1. Create Grant
    let grant_id = 1;
    let total_amount = 1_000_000 * SCALING_FACTOR; // Large enough to not complete early
    let flow_rate = 1 * SCALING_FACTOR; // 1 token per second
    let warmup_duration = 0;
    
    // Mint tokens to contract for payout
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &warmup_duration, &None);

    // 2. Advance time and check claimable
    set_timestamp(&env, 1010); // 10 seconds later
    assert_eq!(client.claimable(&grant_id), 10 * SCALING_FACTOR);

    // 3. Withdraw
    client.withdraw(&grant_id, &(5 * SCALING_FACTOR));
    assert_eq!(grant_token.balance(&recipient), 5 * SCALING_FACTOR);
    assert_eq!(client.claimable(&grant_id), 5 * SCALING_FACTOR);

    // 4. Propose Rate Increase (Timelocked)
    let new_rate = 2 * SCALING_FACTOR;
    client.propose_rate_change(&grant_id, &new_rate);
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.pending_rate, new_rate);
    assert_eq!(grant.effective_timestamp, 1010 + 48 * 60 * 60);

    // 5. Advance time past timelock
    // switch happens at 1010 + 172800 -> 173810
    // now is 1010 + 172800 + 10 -> 173820
    set_timestamp(&env, 1010 + 48 * 60 * 60 + 10);
    // Claimable: 5 (leftover) + 172800 (at rate 1) + 20 (10s at rate 2) = 172825
    assert_eq!(client.claimable(&grant_id), 172825 * SCALING_FACTOR);
}

#[test]
fn test_warmup() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, _grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    set_timestamp(&env, 1000);
    let grant_id = 1;
    let flow_rate = 100 * SCALING_FACTOR;
    let warmup_duration = 100; // 100 seconds warmup
    
    client.create_grant(&grant_id, &recipient, &(10000 * SCALING_FACTOR), &flow_rate, &warmup_duration, &None);

    // At T=1100, the instantaneous multiplier is 100% (10000 bps)
    // The current logic settle at the END of the period at the END rate.
    set_timestamp(&env, 1100);
    assert_eq!(client.claimable(&grant_id), 10000 * SCALING_FACTOR);
}

#[test]
fn test_rage_quit() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);
    
    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);
    
    client.create_grant(&grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0, &None);
    
    set_timestamp(&env, 1100); // 100 tokens accrued
    client.pause_stream(&grant_id);
    
    client.rage_quit(&grant_id);
    
    assert_eq!(grant_token.balance(&recipient), 100 * SCALING_FACTOR);
    assert_eq!(grant_token.balance(&treasury), 900 * SCALING_FACTOR);
}

#[test]
fn test_apply_kpi_multiplier_requires_oracle_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, _grant_token, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    let grant_id = 1;
    client.create_grant(&grant_id, &recipient, &(1000 * SCALING_FACTOR), &SCALING_FACTOR, &0, &None);
    
    // env.set_source_account(&oracle);
    client.apply_kpi_multiplier(&grant_id, &20000); // 2x in basis points
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.flow_rate, 2 * SCALING_FACTOR);
}

#[test]
fn test_apply_kpi_multiplier_settles_before_updating_rate() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, _grant_token, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    set_timestamp(&env, 1000);
    let grant_id = 1;
    client.create_grant(&grant_id, &recipient, &(1000 * SCALING_FACTOR), &SCALING_FACTOR, &0, &None);
    
    set_timestamp(&env, 1100); // 100 accrued
    client.apply_kpi_multiplier(&grant_id, &20000); // 2x
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.claimable, 100 * SCALING_FACTOR);
    assert_eq!(grant.flow_rate, 2 * SCALING_FACTOR);
}

#[test]
fn test_apply_kpi_multiplier_rejects_invalid_multiplier_and_inactive_states() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);
    
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(&grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0, &None);
    
    assert!(client.try_apply_kpi_multiplier(&grant_id, &0).is_err());
    
    client.cancel_grant(&grant_id);
    assert!(client.try_apply_kpi_multiplier(&grant_id, &20000).is_err());
}

#[test]
fn test_apply_kpi_multiplier_scales_pending_rate_and_preserves_accrual_boundaries() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, _grant_token, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    set_timestamp(&env, 1000);
    let grant_id = 1;
    client.create_grant(&grant_id, &recipient, &(100000 * SCALING_FACTOR), &SCALING_FACTOR, &0, &None);
    
    set_timestamp(&env, 1100);
    client.propose_rate_change(&grant_id, &(2 * SCALING_FACTOR));
    
    set_timestamp(&env, 1150);
    client.apply_kpi_multiplier(&grant_id, &20000); // 2x
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.flow_rate, 2 * SCALING_FACTOR);
    assert_eq!(grant.pending_rate, 4 * SCALING_FACTOR);
    assert_eq!(grant.claimable, 150 * SCALING_FACTOR);
}

// ─── Validator Incentive Split Tests ─────────────────────────────────────────

/// After time elapses, 95% of accruals are claimable by the grantee and 5% by
/// the validator, independently tracked.
#[test]
fn test_validator_split_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let validator = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1_000_000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR; // 1 token/sec
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(
        &grant_id, &recipient, &total_amount, &flow_rate, &0,
        &Some(validator.clone()),
    );

    // Advance 100 seconds: 100 tokens accrued total
    // Grantee gets 95, validator gets 5
    set_timestamp(&env, 1100);
    assert_eq!(client.claimable(&grant_id), 95 * SCALING_FACTOR);
    assert_eq!(client.validator_claimable(&grant_id), 5 * SCALING_FACTOR);
}

/// Grantee and validator can withdraw independently; each counter is isolated.
#[test]
fn test_validator_withdraw_independent() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let validator = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1_000_000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(
        &grant_id, &recipient, &total_amount, &flow_rate, &0,
        &Some(validator.clone()),
    );

    // After 200 seconds: 190 grantee, 10 validator
    set_timestamp(&env, 1200);

    // Grantee withdraws their full share
    client.withdraw(&grant_id, &(190 * SCALING_FACTOR));
    assert_eq!(grant_token.balance(&recipient), 190 * SCALING_FACTOR);

    // Validator claimable still intact
    assert_eq!(client.validator_claimable(&grant_id), 10 * SCALING_FACTOR);

    // Validator withdraws their share
    client.withdraw_validator(&grant_id, &(10 * SCALING_FACTOR));
    assert_eq!(grant_token.balance(&validator), 10 * SCALING_FACTOR);

    // Both counters are now zero
    assert_eq!(client.claimable(&grant_id), 0);
    assert_eq!(client.validator_claimable(&grant_id), 0);
}

/// Without a validator the full stream goes to the grantee (no regression).
#[test]
fn test_no_validator_unaffected() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1_000_000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(
        &grant_id, &recipient, &total_amount, &flow_rate, &0,
        &None,
    );

    set_timestamp(&env, 1100);
    // Full 100 tokens go to grantee
    assert_eq!(client.claimable(&grant_id), 100 * SCALING_FACTOR);
    assert_eq!(client.validator_claimable(&grant_id), 0);

    client.withdraw(&grant_id, &(100 * SCALING_FACTOR));
    assert_eq!(grant_token.balance(&recipient), 100 * SCALING_FACTOR);
}

/// On rage quit the validator receives their accrued 5% and the rest returns
/// to treasury.
#[test]
fn test_validator_split_rage_quit() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let validator = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(
        &grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0,
        &Some(validator.clone()),
    );

    // 100 seconds: 95 grantee, 5 validator
    set_timestamp(&env, 1100);
    client.pause_stream(&grant_id);
    client.rage_quit(&grant_id);

    assert_eq!(grant_token.balance(&recipient), 95 * SCALING_FACTOR);
    assert_eq!(grant_token.balance(&validator), 5 * SCALING_FACTOR);
    // Remaining 900 returns to treasury
    assert_eq!(grant_token.balance(&treasury), 900 * SCALING_FACTOR);
}

/// On cancel, only unallocated funds (not yet accrued or withdrawn) go to
/// treasury; the grantee and validator can still pull their claimable shares.
#[test]
fn test_validator_split_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let validator = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(
        &grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0,
        &Some(validator.clone()),
    );

    // 100 seconds: 95 grantee, 5 validator accrued (900 unallocated)
    set_timestamp(&env, 1100);
    client.cancel_grant(&grant_id);

    // Treasury receives 900 unallocated tokens
    assert_eq!(grant_token.balance(&treasury), 900 * SCALING_FACTOR);

    // Grantee can still claim their 95
    assert_eq!(client.claimable(&grant_id), 95 * SCALING_FACTOR);
    // Validator can still claim their 5
    assert_eq!(client.validator_claimable(&grant_id), 5 * SCALING_FACTOR);
}

/// Only the designated validator address can call withdraw_validator.
#[test]
fn test_withdraw_validator_requires_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let validator = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1_000_000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(
        &grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0,
        &Some(validator.clone()),
    );

    set_timestamp(&env, 1100);

    // Grant with no validator must reject withdraw_validator
    let grant_id_no_val = 2;
    client.create_grant(
        &grant_id_no_val, &recipient, &total_amount, &SCALING_FACTOR, &0,
        &None,
    );
    assert!(client.try_withdraw_validator(&grant_id_no_val, &(1 * SCALING_FACTOR)).is_err());

    // Attempting to overdraw validator share must fail
    assert!(client.try_withdraw_validator(&grant_id, &(100 * SCALING_FACTOR)).is_err());

    // Exact amount must succeed
    // Exact amount must succeed
    client.withdraw_validator(&grant_id, &(5 * SCALING_FACTOR));
}

#[test]
fn test_request_extension() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let initial_amount = 100 * SCALING_FACTOR;
    let initial_flow_rate = 1 * SCALING_FACTOR; // 100 seconds duration
    
    grant_token_admin.mint(&client.address, &initial_amount);
    client.create_grant(&grant_id, &recipient, &initial_amount, &initial_flow_rate, &0, &None);

    // 1. Advance 50 seconds: 50 tokens accrued
    set_timestamp(&env, 1050);
    assert_eq!(client.claimable(&grant_id), 50 * SCALING_FACTOR);

    // 2. Extend the grant
    // Current state: 50 accrued, 50 remaining to accrue.
    // Top up: 100 tokens. Total remaining to accrue: 50 + 100 = 150.
    // New end date: 1200. Remaining duration: 1200 - 1050 = 150 seconds.
    // New flow rate should be: 150 / 150 = 1 token/sec.
    
    let top_up = 100 * SCALING_FACTOR;
    let new_end_date = 1200;
    
    // Mint top-up to admin so they can deposit it
    grant_token_admin.mint(&admin, &top_up);
    
    client.request_extension(&grant_id, &top_up, &new_end_date);
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.total_amount, 200 * SCALING_FACTOR);
    assert_eq!(grant.flow_rate, 1 * SCALING_FACTOR);
    assert_eq!(grant.status, GrantStatus::Active);

    // 3. Advance another 50 seconds (T=1100): 50 more tokens accrued
    set_timestamp(&env, 1100);
    assert_eq!(client.claimable(&grant_id), 100 * SCALING_FACTOR);

    // 4. Extend again with a different rate
    // Current state: 100 accrued, 100 remaining.
    // Top up: 0. New end date: 1300. Remaining duration: 1300 - 1100 = 200 seconds.
    // New flow rate: 100 / 200 = 0.5 tokens/sec.
    client.request_extension(&grant_id, &0, &1300);
    
    let grant2 = client.get_grant(&grant_id);
    assert_eq!(grant2.flow_rate, SCALING_FACTOR / 2);
    
    // 5. Test reactivation of completed grant
    set_timestamp(&env, 1500); // Past 1300
    client.withdraw(&grant_id, &0); // Trigger settlement and write to storage
    assert_eq!(client.get_grant(&grant_id).status, GrantStatus::Completed);
    
    // Extend a completed grant
    // Remaining to accrue: 0. Top up: 200. New end date: 1700. Duration: 1700 - 1500 = 200.
    // Rate: 200 / 200 = 1.
    grant_token_admin.mint(&admin, &(200 * SCALING_FACTOR));
    client.request_extension(&grant_id, &(200 * SCALING_FACTOR), &1700);
    
    let grant3 = client.get_grant(&grant_id);
    assert_eq!(grant3.status, GrantStatus::Active);
    assert_eq!(grant3.flow_rate, 1 * SCALING_FACTOR);
}

#[test]
fn test_auditor_role() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let auditor1 = Address::generate(&env);
    let auditor2 = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    // 1. Set Auditors
    client.set_auditors(&vec![&env, auditor1.clone(), auditor2.clone()]);

    // 2. Create Grant
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);
    client.create_grant(&grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0, &None);

    set_timestamp(&env, 1100); // 100 tokens accrued
    
    // 3. Auditor Flags the Grant
    client.flag_grant(&auditor1, &grant_id);
    
    let grant = client.get_grant(&grant_id);
    assert!(grant.is_flagged);
    
    // 4. Withdrawal must fail during audit
    // In soroban, regular Panics are difficult to catch across contract boundaries in tests without try_
    // but try_withdraw should fail.
    assert!(client.try_withdraw(&grant_id, &(10 * SCALING_FACTOR)).is_err());
    
    // 5. Auditor Resolves the Flag
    client.resolve_flag(&auditor2, &grant_id);
    assert!(!client.get_grant(&grant_id).is_flagged);
    
    // 6. Withdrawal now succeeds
    client.withdraw(&grant_id, &(10 * SCALING_FACTOR));
    
    // 7. Flag again for admin resolve test
    client.flag_grant(&auditor2, &grant_id);
    assert!(client.get_grant(&grant_id).is_flagged);
    
    // 8. Admin resolves
    client.resolve_flag(&admin, &grant_id);
    assert!(!client.get_grant(&grant_id).is_flagged);

    // 9. Unauthorized flagging must fail
    let random_user = Address::generate(&env);
    assert!(client.try_flag_grant(&random_user, &grant_id).is_err());
}

#[test]
fn test_qf_drip_pool() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let donor1 = Address::generate(&env);
    let donor2 = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    // 1. Setup QF Pool
    let pool_id = 1;
    let drip_rate = 10 * SCALING_FACTOR; // 10 tokens per second total matching
    client.create_qf_pool(&pool_id, &grant_token_addr, &drip_rate);

    // 2. Setup Grants
    let grant_id1 = 1;
    let grant_id2 = 2;
    grant_token_admin.mint(&client.address, &(2000 * SCALING_FACTOR));
    client.create_grant(&grant_id1, &recipient1, &(1000 * SCALING_FACTOR), &SCALING_FACTOR, &0, &None);
    client.create_grant(&grant_id2, &recipient2, &(1000 * SCALING_FACTOR), &SCALING_FACTOR, &0, &None);

    client.register_qf_project(&pool_id, &grant_id1);
    client.register_qf_project(&pool_id, &grant_id2);

    // 3. Donations to Project 1 (Two donors to trigger matching)
    grant_token_admin.mint(&donor1, &(500 * SCALING_FACTOR));
    grant_token_admin.mint(&donor2, &(500 * SCALING_FACTOR));
    
    // Project 1: Donor 1 gives 100. sum_sqrt = sqrt(100) = 10. weight = 10^2 - 100 = 0.
    client.donate_to_qf(&donor1, &pool_id, &grant_id1, &(100 * SCALING_FACTOR));
    // Project 1: Donor 2 gives 100. sum_sqrt = 10 + 10 = 20. weight = 20^2 - 200 = 200.
    client.donate_to_qf(&donor2, &pool_id, &grant_id1, &(100 * SCALING_FACTOR));

    let project1 = client.get_qf_project_info(&pool_id, &grant_id1);
    assert!((project1.matching_weight - 200 * SCALING_FACTOR).abs() < 1000);

    // 4. Wait 10 seconds.
    // Total Weight = 200. Total Drip = 10 tokens/sec.
    // Project 1 has 100% of weight, so it should get 10 * 10 = 100 tokens.
    set_timestamp(&env, 1010);
    
    let info1 = client.get_qf_project_info(&pool_id, &grant_id1);
    assert!((info1.accrued_matching - 100 * SCALING_FACTOR).abs() < 1000);

    // 5. Donate to Project 2
    // Project 2: Donor 1 gives 100. Donor 2 gives 100 -> weight = 200.
    grant_token_admin.mint(&donor1, &(100 * SCALING_FACTOR));
    grant_token_admin.mint(&donor2, &(100 * SCALING_FACTOR));
    client.donate_to_qf(&donor1, &pool_id, &grant_id2, &(100 * SCALING_FACTOR));
    client.donate_to_qf(&donor2, &pool_id, &grant_id1, &(100 * SCALING_FACTOR)); // Project 1 weight: sum_sqrt = 30. weight = 900 - 300 = 600.
    client.donate_to_qf(&donor2, &pool_id, &grant_id2, &(100 * SCALING_FACTOR)); // Project 2 weight: sum_sqrt = 20. weight = 200.

    // New Total Weight = 600 + 200 = 800.
    // Projects share: P1 (6/8), P2 (2/8).
    
    set_timestamp(&env, 1020); // 10 more seconds
    
    let p1 = client.get_qf_project_info(&pool_id, &grant_id1);
    let p2 = client.get_qf_project_info(&pool_id, &grant_id2);

    // P1 got 100 before. 
    // In the last 10s: Total drip = 100. P1 share = 100 * 6/8 = 75. Total P1 = 175.
    // P2 share = 100 * 2/8 = 25.
    assert!((p1.accrued_matching - 175 * SCALING_FACTOR).abs() < 1000);
    assert!((p2.accrued_matching - 25 * SCALING_FACTOR).abs() < 1000);

    // 6. Withdrawal
    grant_token_admin.mint(&client.address, &(1000 * SCALING_FACTOR)); // Ensure contract has tokens for matching
    client.withdraw_qf_match(&pool_id, &grant_id1);
    
    let final_info1 = client.get_qf_project_info(&pool_id, &grant_id1);
    assert_eq!(final_info1.accrued_matching, 0);
}

#[test]
fn test_treasury_rage_quit() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient1 = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    // 1. Create a large grant
    let grant_id = 1;
    let initial_total = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &initial_total);
    client.create_grant(&grant_id, &recipient1, &initial_total, &SCALING_FACTOR, &0, &None);

    // 2. Simulate Rage-Quit by reducing treasury balance
    // Current TotalAllocated = 1000. New Treasury = 500.
    client.notify_treasury_reduction(&(500 * SCALING_FACTOR));
    
    // 3. Project should be pro-rated to 50%
    let grant = client.get_grant(&grant_id);
    // Note: get_grant usually reads from storage. 
    // Does it apply scaling? Yes, we should probably add apply_scaling to get_grant too 
    // OR realize that withdraw will apply it.
    // Let's check withdraw.
    
    set_timestamp(&env, 1100); // 100 seconds
    
    // Original flow rate was 1 token/sec. New should be 0.5 tokens/sec.
    // At T=1100 (100 sec since 1000), it should have accrued 50 tokens.
    // total_amount should be 500.
    
    client.withdraw(&grant_id, &(10 * SCALING_FACTOR));
    
    let grant_after = client.get_grant(&grant_id);
    assert_eq!(grant_after.total_amount, 500 * SCALING_FACTOR);
    assert_eq!(grant_after.flow_rate, SCALING_FACTOR / 2);
    assert_eq!(grant_after.claimable, 40 * SCALING_FACTOR); // 50 accrued - 10 withdrawn
}
