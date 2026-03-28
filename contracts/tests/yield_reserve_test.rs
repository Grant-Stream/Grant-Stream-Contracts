#[test]
fn test_yield_allocation_with_safety() {
    let env = Env::default();
    env.mock_all_auths();

    // Setup grants with streaming obligations
    // Allocate 40% to BENJI
    client.allocate_to_yield(&benji_address, &40u32);

    assert!(client.check_liquidity_safety());

    let yield_earned = client.harvest_yield(&benji_address);
    assert!(yield_earned > 0);
}