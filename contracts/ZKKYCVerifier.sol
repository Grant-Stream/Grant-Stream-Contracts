// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/access/Ownable.sol";

/**
 * @title ZKKYCVerifier
 * @notice Zero-Knowledge KYC Verification Bridge.
 *
 *         Proves a grantee is a "Real, Verified Person" without storing any
 *         personal identity data (names, passport numbers, etc.) on-chain.
 *
 *         Off-chain flow:
 *           1. The grantee presents their ID to a trusted KYC provider
 *              (e.g. Stellar Aid Assist).
 *           2. The provider runs a ZK circuit that produces a commitment hash —
 *              a proof that verification passed, with no recoverable PII.
 *           3. The trusted `verifier` address calls {verifyAddress} with that
 *              hash.  Only the hash is stored; no personal data ever touches
 *              the chain.
 *           4. GrantStream calls {isVerified} before funding to ensure the DAO
 *              only supports real, verified people.
 *
 * @dev The zero value (bytes32(0)) is the sentinel for "unverified" so it must
 *      never be accepted as a valid proof hash.
 */
contract ZKKYCVerifier is Ownable {
    // ─── State ────────────────────────────────────────────────────────────────

    /// @notice Trusted off-chain verifier relay (e.g. Stellar Aid Assist bridge).
    address public verifier;

    /// @notice Proof-of-verification commitment hash per grantee address.
    ///         bytes32(0) means unverified.
    mapping(address => bytes32) public proofHashes;

    // ─── Events ───────────────────────────────────────────────────────────────

    event AddressVerified(address indexed account, bytes32 indexed proofHash);
    event VerificationRevoked(address indexed account);
    event VerifierUpdated(address indexed oldVerifier, address indexed newVerifier);

    // ─── Constructor ──────────────────────────────────────────────────────────

    /**
     * @param _verifier Trusted relay address that submits ZK proof hashes.
     */
    constructor(address _verifier) Ownable(msg.sender) {
        require(_verifier != address(0), "ZKKYCVerifier: zero verifier");
        verifier = _verifier;
    }

    // ─── Verifier-only mutators ───────────────────────────────────────────────

    /**
     * @notice Record a ZK proof-of-verification commitment for `account`.
     *
     *         The `proofHash` is a commitment produced by the off-chain ZK
     *         circuit — it proves identity was verified without encoding any
     *         recoverable personal data.
     *
     * @param account   Grantee address to mark as verified.
     * @param proofHash Non-zero commitment hash from the ZK provider.
     */
    function verifyAddress(address account, bytes32 proofHash) external {
        require(msg.sender == verifier, "ZKKYCVerifier: caller is not verifier");
        require(account != address(0), "ZKKYCVerifier: zero account");
        require(proofHash != bytes32(0), "ZKKYCVerifier: zero proof hash");

        proofHashes[account] = proofHash;
        emit AddressVerified(account, proofHash);
    }

    /**
     * @notice Revoke verification for `account` (e.g. fraud detected or
     *         credential expired).  Only the trusted verifier may revoke.
     *
     * @param account Grantee address whose verification is revoked.
     */
    function revokeVerification(address account) external {
        require(msg.sender == verifier, "ZKKYCVerifier: caller is not verifier");
        require(proofHashes[account] != bytes32(0), "ZKKYCVerifier: not verified");

        delete proofHashes[account];
        emit VerificationRevoked(account);
    }

    // ─── Owner-only admin ─────────────────────────────────────────────────────

    /**
     * @notice Rotate the trusted verifier address (e.g. key compromise or
     *         provider migration).  Only the contract owner may call this.
     *
     * @param _newVerifier Replacement verifier address.
     */
    function setVerifier(address _newVerifier) external onlyOwner {
        require(_newVerifier != address(0), "ZKKYCVerifier: zero verifier");
        emit VerifierUpdated(verifier, _newVerifier);
        verifier = _newVerifier;
    }

    // ─── View ─────────────────────────────────────────────────────────────────

    /**
     * @notice Returns `true` when `account` holds a non-zero proof hash,
     *         meaning the off-chain verifier confirmed their identity.
     *
     * @param account Address to query.
     */
    function isVerified(address account) external view returns (bool) {
        return proofHashes[account] != bytes32(0);
    }
}
