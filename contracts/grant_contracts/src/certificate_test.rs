#[test]
fn test_mint_completion_certificate_on_full_payout() {
    let env = Env::default();
    env.mock_all_auths();

    // Setup grant that reaches 100%
    // Call finish_stream
    let token_id = client.mint_completion_certificate(...);

    assert!(token_id > 0);
    // Verify metadata contains correct grant_id, repo_url, etc.
}