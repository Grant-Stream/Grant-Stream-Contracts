//! Cleanup Bounty — "Clean Ledger" incentive (Issue: optimization/economics)
//!
//! Any caller may invoke `finalize_and_purge` on a 100%-completed grant.
//! The contract:
//!   1. Verifies the grant is `Completed` and has zero remaining balance.
//!   2. Removes the grant's persistent storage entry (reduces state bloat).
//!   3. Pays the caller a small bounty equal to `CLEANUP_BOUNTY_BPS` of the
//!      platform fee that was collected when the grant was created/funded.
//!      The bounty is sourced from the treasury, so the treasury must hold
//!      sufficient funds.
//!
//! Constants are intentionally conservative: 5 bps (0.05 %) of the platform
//! fee keeps the incentive meaningful without draining the treasury.

#![allow(unused)]

use soroban_sdk::{symbol_short, token, Address, Env};

use crate::{DataKey, Error, Grant, GrantContract, GrantStatus};

/// Bounty paid to the cleanup caller, expressed as basis points of the
/// platform fee that was collected for this grant.
/// 5 bps = 0.05 % of the platform fee.
const CLEANUP_BOUNTY_BPS: i128 = 5;

impl GrantContract {
    /// Remove a fully-completed, zero-balance grant from on-chain storage and
    /// reward the caller with a small bounty sourced from the treasury.
    ///
    /// # Preconditions
    /// - `grant.status == GrantStatus::Completed`
    /// - `grant.remaining_balance == 0`  (all funds have been disbursed)
    ///
    /// # Returns
    /// The bounty amount transferred to the caller (in token stroops).
    pub fn finalize_and_purge(env: Env, grant_id: u64, caller: Address) -> Result<i128, Error> {
        caller.require_auth();

        let grant: Grant = env
            .storage()
            .instance()
            .get(&DataKey::Grant(grant_id))
            .ok_or(Error::GrantNotFound)?;

        // Only fully-completed grants may be purged.
        if grant.status != GrantStatus::Completed {
            return Err(Error::InvalidState);
        }

        // Guard: refuse to purge if any balance remains (shouldn't happen for
        // Completed grants, but be defensive).
        if grant.remaining_balance != 0 {
            return Err(Error::InvalidState);
        }

        // --- Bounty calculation ---
        // platform_fee_bps is stored at initialisation; fall back to 0 if unset.
        let platform_fee_bps: i128 = env
            .storage()
            .instance()
            .get(&DataKey::PlatformFeeBps)
            .unwrap_or(0i32) as i128;

        // platform_fee = total_amount * platform_fee_bps / 10_000
        let platform_fee = (grant.total_amount * platform_fee_bps) / 10_000;

        // bounty = platform_fee * CLEANUP_BOUNTY_BPS / 10_000
        let bounty = (platform_fee * CLEANUP_BOUNTY_BPS) / 10_000;

        // --- Transfer bounty from treasury to caller ---
        if bounty > 0 {
            let token_addr: Address = env
                .storage()
                .instance()
                .get(&DataKey::GrantToken)
                .ok_or(Error::NotInitialized)?;
            let treasury: Address = env
                .storage()
                .instance()
                .get(&DataKey::Treasury)
                .ok_or(Error::NotInitialized)?;

            token::Client::new(&env, &token_addr).transfer(
                &treasury,
                &caller,
                &bounty,
            );
        }

        // --- Purge grant storage (the core state-bloat reduction) ---
        env.storage().instance().remove(&DataKey::Grant(grant_id));

        env.events().publish(
            (symbol_short!("purged"), grant_id),
            (caller, bounty),
        );

        Ok(bounty)
    }
}
