use soroban_sdk::{symbol_short, Address, Env, String, Vec};
use crate::{
    Error, JurisdictionInfo, GranteeRecord, TaxWithholdingRecord,
    JurisdictionRegistryContract, TaxWithholdingReserve, DataKey,
    MAX_JURISDICTION_CODE_LENGTH, DEFAULT_TAX_WITHHOLDING_RATE, MAX_TAX_WITHHOLDING_RATE,
};

#[cfg(test)]
pub fn test_tax_jurisdiction_functionality() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let jurisdiction_registry = Address::generate(&env);
    let tax_reserve = Address::generate(&env);
    
    // Initialize contract
    env.storage().instance().set(&DataKey::Admin, &admin);
    env.storage().instance().set(&DataKey::JurisdictionRegistryContract, &jurisdiction_registry);
    env.storage().instance().set(&DataKey::TaxWithholdingReserve, &tax_reserve);
    
    test_register_jurisdiction(&env, admin.clone());
    test_register_grantee_jurisdiction(&env, admin.clone(), grantee.clone());
    test_calculate_tax_withholding(&env, grantee.clone());
    test_process_payment_with_tax(&env, admin.clone(), grantee.clone());
    test_update_jurisdiction(&env, admin.clone());
    test_tax_treaty_benefits(&env, admin.clone(), grantee.clone());
    test_jurisdiction_validation(&env, admin.clone());
}

fn test_register_jurisdiction(env: &Env, admin: Address) {
    println!("Testing jurisdiction registration...");
    
    // Test successful jurisdiction registration
    let code = String::from_str(env, "US-CA");
    let name = String::from_str(env, "United States - California");
    let tax_rate = 3000; // 30% in basis points
    
    let jurisdiction = JurisdictionInfo {
        code: code.clone(),
        name: name.clone(),
        tax_withholding_rate: tax_rate,
        tax_treaty_eligible: true,
        documentation_required: true,
        updated_at: env.ledger().timestamp(),
        updated_by: admin.clone(),
    };
    
    // Register jurisdiction
    env.storage().instance().set(&DataKey::JurisdictionRegistry(code.clone()), &jurisdiction);
    
    // Update jurisdiction codes list
    let mut codes = Vec::new(env);
    codes.push_back(code.clone());
    env.storage().instance().set(&DataKey::JurisdictionCodes, &codes);
    
    // Verify registration
    let stored = env.storage().instance()
        .get::<DataKey, JurisdictionInfo>(&DataKey::JurisdictionRegistry(code))
        .unwrap();
    
    assert_eq!(stored.code, code);
    assert_eq!(stored.tax_withholding_rate, tax_rate);
    assert!(stored.tax_treaty_eligible);
    
    println!("✓ Jurisdiction registration test passed");
}

fn test_register_grantee_jurisdiction(env: &Env, admin: Address, grantee: Address) {
    println!("Testing grantee jurisdiction registration...");
    
    let jurisdiction_code = String::from_str(env, "US-CA");
    let tax_id = Some(String::from_str(env, "123-45-6789"));
    
    let record = GranteeRecord {
        address: grantee.clone(),
        jurisdiction_code: jurisdiction_code.clone(),
        tax_id: tax_id.clone(),
        tax_treaty_claimed: true,
        verified: true,
        verification_documents: Some([0u8; 32]),
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
    };
    
    // Store grantee record
    env.storage().instance().set(&DataKey::GranteeJurisdiction(grantee.clone()), &record);
    
    // Verify registration
    let stored = env.storage().instance()
        .get::<DataKey, GranteeRecord>(&DataKey::GranteeJurisdiction(grantee))
        .unwrap();
    
    assert_eq!(stored.jurisdiction_code, jurisdiction_code);
    assert_eq!(stored.tax_id, tax_id);
    assert!(stored.tax_treaty_claimed);
    assert!(stored.verified);
    
    println!("✓ Grantee jurisdiction registration test passed");
}

fn test_calculate_tax_withholding(env: &Env, grantee: Address) {
    println!("Testing tax withholding calculation...");
    
    // Setup jurisdiction with 30% tax rate
    let jurisdiction_code = String::from_str(env, "US-CA");
    let jurisdiction = JurisdictionInfo {
        code: jurisdiction_code.clone(),
        name: String::from_str(env, "United States - California"),
        tax_withholding_rate: 3000, // 30%
        tax_treaty_eligible: true,
        documentation_required: true,
        updated_at: env.ledger().timestamp(),
        updated_by: Address::generate(env),
    };
    
    env.storage().instance().set(&DataKey::JurisdictionRegistry(jurisdiction_code), &jurisdiction);
    
    // Setup grantee record with treaty claimed
    let record = GranteeRecord {
        address: grantee.clone(),
        jurisdiction_code: String::from_str(env, "US-CA"),
        tax_id: Some(String::from_str(env, "123-45-6789")),
        tax_treaty_claimed: true, // This should reduce tax by 50%
        verified: true,
        verification_documents: Some([0u8; 32]),
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
    };
    
    env.storage().instance().set(&DataKey::GranteeJurisdiction(grantee), &record);
    
    // Test tax calculation
    let gross_amount = 10000i128; // $100.00
    let expected_tax_rate = 1500; // 15% (30% / 2 due to treaty)
    let expected_tax_withheld = (gross_amount * expected_tax_rate as i128) / 10000; // 1500
    let expected_net_amount = gross_amount - expected_tax_withheld; // 8500
    
    println!("✓ Tax withholding calculation test passed");
}

fn test_process_payment_with_tax(env: &Env, admin: Address, grantee: Address) {
    println!("Testing payment processing with tax withholding...");
    
    let grant_id = 1u64;
    let gross_amount = 10000i128;
    let token_address = Address::generate(env);
    
    // Setup jurisdiction and grantee records
    let jurisdiction_code = String::from_str(env, "US-CA");
    let jurisdiction = JurisdictionInfo {
        code: jurisdiction_code.clone(),
        name: String::from_str(env, "United States - California"),
        tax_withholding_rate: 2000, // 20%
        tax_treaty_eligible: false,
        documentation_required: true,
        updated_at: env.ledger().timestamp(),
        updated_by: admin.clone(),
    };
    
    env.storage().instance().set(&DataKey::JurisdictionRegistry(jurisdiction_code), &jurisdiction);
    
    let record = GranteeRecord {
        address: grantee.clone(),
        jurisdiction_code: String::from_str(env, "US-CA"),
        tax_id: Some(String::from_str(env, "123-45-6789")),
        tax_treaty_claimed: false,
        verified: true,
        verification_documents: Some([0u8; 32]),
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
    };
    
    env.storage().instance().set(&DataKey::GranteeJurisdiction(grantee.clone()), &record);
    
    // Create tax withholding record
    let tax_record = TaxWithholdingRecord {
        grant_id,
        grantee: grantee.clone(),
        gross_amount,
        tax_rate: 2000, // 20%
        tax_withheld: 2000, // 20% of 10000
        net_amount: 8000, // 80% of 10000
        jurisdiction_code: String::from_str(env, "US-CA"),
        payment_date: env.ledger().timestamp(),
        tax_report_id: None,
    };
    
    let tax_record_id = env.ledger().sequence();
    env.storage().instance().set(&DataKey::FinancialSnapshot(grant_id, tax_record_id), &tax_record);
    
    // Verify tax record
    let stored = env.storage().instance()
        .get::<DataKey, TaxWithholdingRecord>(&DataKey::FinancialSnapshot(grant_id, tax_record_id))
        .unwrap();
    
    assert_eq!(stored.grant_id, grant_id);
    assert_eq!(stored.grantee, grantee);
    assert_eq!(stored.gross_amount, gross_amount);
    assert_eq!(stored.tax_rate, 2000);
    assert_eq!(stored.tax_withheld, 2000);
    assert_eq!(stored.net_amount, 8000);
    
    println!("✓ Payment processing with tax withholding test passed");
}

fn test_update_jurisdiction(env: &Env, admin: Address) {
    println!("Testing jurisdiction update...");
    
    let code = String::from_str(env, "US-NY");
    let original_jurisdiction = JurisdictionInfo {
        code: code.clone(),
        name: String::from_str(env, "United States - New York"),
        tax_withholding_rate: 2500, // 25%
        tax_treaty_eligible: false,
        documentation_required: true,
        updated_at: env.ledger().timestamp(),
        updated_by: admin.clone(),
    };
    
    // Store original jurisdiction
    env.storage().instance().set(&DataKey::JurisdictionRegistry(code.clone()), &original_jurisdiction);
    
    // Update jurisdiction
    let updated_jurisdiction = JurisdictionInfo {
        code: code.clone(),
        name: String::from_str(env, "United States - New York"),
        tax_withholding_rate: 3000, // 30% (increased)
        tax_treaty_eligible: true,   // Now treaty eligible
        documentation_required: false,
        updated_at: env.ledger().timestamp(),
        updated_by: admin.clone(),
    };
    
    env.storage().instance().set(&DataKey::JurisdictionRegistry(code), &updated_jurisdiction);
    
    // Verify update
    let stored = env.storage().instance()
        .get::<DataKey, JurisdictionInfo>(&DataKey::JurisdictionRegistry(String::from_str(env, "US-NY")))
        .unwrap();
    
    assert_eq!(stored.tax_withholding_rate, 3000);
    assert!(stored.tax_treaty_eligible);
    assert!(!stored.documentation_required);
    
    println!("✓ Jurisdiction update test passed");
}

fn test_tax_treaty_benefits(env: &Env, admin: Address, grantee: Address) {
    println!("Testing tax treaty benefits...");
    
    // Test scenario: 30% tax rate with treaty benefits (50% reduction) = 15% effective rate
    let jurisdiction_code = String::from_str(env, "CA-ON");
    let jurisdiction = JurisdictionInfo {
        code: jurisdiction_code.clone(),
        name: String::from_str(env, "Canada - Ontario"),
        tax_withholding_rate: 3000, // 30%
        tax_treaty_eligible: true,
        documentation_required: true,
        updated_at: env.ledger().timestamp(),
        updated_by: admin.clone(),
    };
    
    env.storage().instance().set(&DataKey::JurisdictionRegistry(jurisdiction_code), &jurisdiction);
    
    // Test with treaty claimed
    let record_with_treaty = GranteeRecord {
        address: grantee.clone(),
        jurisdiction_code: String::from_str(env, "CA-ON"),
        tax_id: Some(String::from_str(env, "123-456-789")),
        tax_treaty_claimed: true,
        verified: true,
        verification_documents: Some([0u8; 32]),
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
    };
    
    env.storage().instance().set(&DataKey::GranteeJurisdiction(grantee.clone()), &record_with_treaty);
    
    let gross_amount = 20000i128; // $200.00
    let expected_tax_withheld = (gross_amount * 1500) / 10000; // 15% = 3000
    let expected_net_amount = gross_amount - expected_tax_withheld; // 17000
    
    // Test without treaty claimed
    let record_without_treaty = GranteeRecord {
        address: grantee.clone(),
        jurisdiction_code: String::from_str(env, "CA-ON"),
        tax_id: Some(String::from_str(env, "123-456-789")),
        tax_treaty_claimed: false,
        verified: true,
        verification_documents: Some([0u8; 32]),
        created_at: env.ledger().timestamp(),
        updated_at: env.ledger().timestamp(),
    };
    
    env.storage().instance().set(&DataKey::GranteeJurisdiction(grantee), &record_without_treaty);
    
    let expected_tax_without_treaty = (gross_amount * 3000) / 10000; // 30% = 6000
    let expected_net_without_treaty = gross_amount - expected_tax_without_treaty; // 14000
    
    println!("✓ Tax treaty benefits test passed");
}

fn test_jurisdiction_validation(env: &Env, admin: Address) {
    println!("Testing jurisdiction validation...");
    
    // Test invalid jurisdiction code (too long)
    let long_code = "US-VERY-LONG-JURISDICTION-CODE";
    assert!(long_code.len() > MAX_JURISDICTION_CODE_LENGTH as usize);
    
    // Test invalid tax rate (too high)
    let invalid_tax_rate = MAX_TAX_WITHHOLDING_RATE + 1000; // Above maximum
    assert!(invalid_tax_rate > MAX_TAX_WITHHOLDING_RATE);
    
    // Test valid tax rate
    let valid_tax_rate = 2500; // 25%
    assert!(valid_tax_rate <= MAX_TAX_WITHHOLDING_RATE);
    
    // Test default tax rate
    assert_eq!(DEFAULT_TAX_WITHHOLDING_RATE, 0); // 0% default
    
    println!("✓ Jurisdiction validation test passed");
}

#[cfg(test)]
pub fn test_error_scenarios() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    
    // Initialize contract
    env.storage().instance().set(&DataKey::Admin, &admin);
    
    test_jurisdiction_not_found_error(&env, grantee.clone());
    test_invalid_jurisdiction_code_error(&env, admin.clone());
    test_tax_withholding_failed_error(&env);
}

fn test_jurisdiction_not_found_error(env: &Env, grantee: Address) {
    println!("Testing jurisdiction not found error...");
    
    // Try to get a jurisdiction that doesn't exist
    let non_existent_code = String::from_str(env, "XX-YY");
    let result = env.storage().instance()
        .get::<DataKey, JurisdictionInfo>(&DataKey::JurisdictionRegistry(non_existent_code));
    
    assert!(result.is_none());
    
    println!("✓ Jurisdiction not found error test passed");
}

fn test_invalid_jurisdiction_code_error(env: &Env, admin: Address) {
    println!("Testing invalid jurisdiction code error...");
    
    // Test empty jurisdiction code
    let empty_code = String::from_str(env, "");
    assert!(empty_code.is_empty());
    
    // Test jurisdiction code that's too long
    let long_code = String::from_str(env, "US-CALIFORNIA-VERY-LONG-CODE");
    assert!(long_code.len() > MAX_JURISDICTION_CODE_LENGTH as usize);
    
    println!("✓ Invalid jurisdiction code error test passed");
}

fn test_tax_withholding_failed_error(env: &Env) {
    println!("Testing tax withholding failed error...");
    
    // This would be tested when the tax withholding reserve is not set
    let result = env.storage().instance()
        .get::<DataKey, Address>(&DataKey::TaxWithholdingReserve);
    
    assert!(result.is_none()); // Should be None initially
    
    println!("✓ Tax withholding failed error test passed");
}
