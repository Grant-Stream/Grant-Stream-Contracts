use soroban_sdk::{contract, contractimpl, Address, Env, String, Symbol, BytesN, Map};
use crate::grant_contract::GrantId; // Import from your main grant contract

#[derive(Clone)]
pub struct CertificateMetadata {
    pub grant_id: GrantId,
    pub dao: Address,
    pub grantee: Address,
    pub completion_date: u64,
    pub total_streamed: u128,
    pub repo_url: String,
    pub description: String,
}

pub trait CertificateMinterTrait {
    fn mint_completion_certificate(
        env: Env,
        grant_id: GrantId,
        dao: Address,
        grantee: Address,
        total_streamed: u128,
        repo_url: String,
        description: String,
    ) -> u32; // Returns token ID

    fn get_certificate_metadata(env: Env, token_id: u32) -> CertificateMetadata;
    fn is_completed_certificate(env: Env, token_id: u32) -> bool;
}

#[contract]
pub struct CertificateMinter;

#[contractimpl]
impl CertificateMinterTrait for CertificateMinter {
    fn mint_completion_certificate(
        env: Env,
        grant_id: GrantId,
        dao: Address,
        grantee: Address,
        total_streamed: u128,
        repo_url: String,
        description: String,
    ) -> u32 {
        // Only callable by the Grant Contract (or DAO admin)
        // Add authorization check here (e.g. require_auth from grant contract address)

        let token_id = Self::sequential_mint(&env, &grantee); // Using OZ-style sequential mint

        let metadata: Map<String, String> = Map::new(&env);
        metadata.set(String::from_str(&env, "name"), String::from_str(&env, "Grant Completion Certificate"));
        metadata.set(String::from_str(&env, "grant_id"), String::from_str(&env, &grant_id.to_string()));
        metadata.set(String::from_str(&env, "dao"), String::from_str(&env, &dao.to_string()));
        metadata.set(String::from_str(&env, "grantee"), String::from_str(&env, &grantee.to_string()));
        metadata.set(String::from_str(&env, "completion_date"), String::from_str(&env, &env.ledger().timestamp().to_string()));
        metadata.set(String::from_str(&env, "total_streamed"), String::from_str(&env, &total_streamed.to_string()));
        metadata.set(String::from_str(&env, "repo_url"), repo_url.clone());
        metadata.set(String::from_str(&env, "description"), description);

        // Store metadata (you can use IPFS hash in production for off-chain JSON)
        Self::set_token_metadata(&env, token_id, metadata);

        env.events().publish(
            (Symbol::new(&env, "certificate_minted"),),
            (token_id, grant_id, grantee)
        );

        token_id
    }

    // Additional view functions...
    fn get_certificate_metadata(env: Env, token_id: u32) -> CertificateMetadata { ... }
}