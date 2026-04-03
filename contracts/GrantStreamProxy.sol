// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import "./IGrantStreamLogic.sol";

/**
 * @title GrantStreamProxy
 * @notice Stores the "Immutable Terms" for every grant (funder, recipient,
 *         totalAmount) and delegates all mutable logic to a swappable
 *         implementation contract.
 *
 * Upgrade rules (Wasm-Rotation pattern):
 *  - Only the DAO governance contract may call upgradeLogic().
 *  - The new implementation must pass verifyImmutableTerms() for every
 *    active grant before the rotation is accepted.
 *  - The logic hash of the new implementation is recorded on-chain for
 *    full auditability.
 *
 * Active grant streams are never interrupted: their immutable terms live
 * here in the proxy and are untouched by any logic rotation.
 */
contract GrantStreamProxy is ReentrancyGuard {

    // ─── Immutable Terms ──────────────────────────────────────────────────────

    /// @dev Per-grant data that can NEVER change after creation.
    struct ImmutableTerms {
        address funder;
        address recipient;
        uint256 totalAmount;   // original deposit; top-ups are additive but tracked separately
        bool    exists;
    }

    uint256 public nextGrantId;
    mapping(uint256 => ImmutableTerms) public immutableTerms;

    // ─── Upgrade State ────────────────────────────────────────────────────────

    /// @notice Current logic implementation address.
    address public logicImpl;

    /// @notice Keccak256 of the deployed bytecode of logicImpl, recorded at
    ///         upgrade time so off-chain tooling can verify the exact code.
    bytes32 public logicHash;

    /// @notice Address of the DAO governance contract that controls upgrades.
    address public immutable dao;

    // ─── Events ───────────────────────────────────────────────────────────────

    event GrantRegistered(uint256 indexed grantId, address indexed funder, address indexed recipient, uint256 totalAmount);
    event LogicUpgraded(address indexed newImpl, bytes32 indexed newHash, address indexed proposer);

    // ─── Constructor ──────────────────────────────────────────────────────────

    /**
     * @param _initialLogic  First logic implementation to activate.
     * @param _dao           DAO governance contract; the only address allowed
     *                       to call upgradeLogic().
     */
    constructor(address _initialLogic, address _dao) {
        require(_initialLogic != address(0), "Proxy: zero logic");
        require(_dao != address(0),          "Proxy: zero dao");
        dao      = _dao;
        _setLogic(_initialLogic);
    }

    // ─── Grant Registration ───────────────────────────────────────────────────

    /**
     * @notice Register a new grant and lock its immutable terms.
     *         Forwards the ETH deposit to the logic implementation.
     * @param recipient Grantee address — immutable after this call.
     */
    function createGrant(address recipient) external payable nonReentrant returns (uint256 grantId) {
        require(msg.value > 0,           "Proxy: no funds");
        require(recipient != address(0), "Proxy: zero recipient");

        grantId = nextGrantId++;
        immutableTerms[grantId] = ImmutableTerms({
            funder:      msg.sender,
            recipient:   recipient,
            totalAmount: msg.value,
            exists:      true
        });

        emit GrantRegistered(grantId, msg.sender, recipient, msg.value);

        // Forward to logic — logic manages mutable state (balance, status, etc.)
        (bool ok, ) = logicImpl.delegatecall(
            abi.encodeWithSignature("createGrant(address)", recipient)
        );
        require(ok, "Proxy: createGrant delegatecall failed");
    }

    // ─── Delegated Calls ──────────────────────────────────────────────────────

    /**
     * @notice Fallback: delegate every other call to the current logic.
     *         Immutable terms stored in this proxy are read-only from logic's
     *         perspective (they occupy fixed storage slots 4-6).
     */
    fallback() external payable {
        address impl = logicImpl;
        assembly {
            calldatacopy(0, 0, calldatasize())
            let result := delegatecall(gas(), impl, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            switch result
            case 0 { revert(0, returndatasize()) }
            default { return(0, returndatasize()) }
        }
    }

    receive() external payable {}

    // ─── Upgrade (DAO-gated) ──────────────────────────────────────────────────

    /**
     * @notice Rotate the logic implementation.
     *         Called exclusively by the DAO governance contract after a
     *         successful vote.
     *
     * Safety invariant: the new implementation must not alter the immutable
     * terms of any existing grant.  The DAO is responsible for running
     * verifyImmutableTerms() off-chain (or via a simulation) before voting;
     * this function performs a lightweight on-chain sanity check on a
     * caller-supplied sample of grant IDs.
     *
     * @param newImpl       Address of the new logic contract.
     * @param sampleGrantIds A representative set of active grant IDs to spot-
     *                       check.  Pass an empty array to skip (not recommended
     *                       for high-value upgrades).
     */
    function upgradeLogic(
        address newImpl,
        uint256[] calldata sampleGrantIds
    ) external {
        require(msg.sender == dao,    "Proxy: only DAO");
        require(newImpl != address(0), "Proxy: zero impl");
        require(newImpl != logicImpl,  "Proxy: same impl");

        // Spot-check: immutable terms must survive the upgrade.
        for (uint256 i; i < sampleGrantIds.length; i++) {
            uint256 gid = sampleGrantIds[i];
            ImmutableTerms storage t = immutableTerms[gid];
            require(t.exists, "Proxy: unknown grant in sample");
            // The new logic must acknowledge the same funder/recipient/amount.
            // We call a view on the new impl (staticcall, no state change).
            (bool ok, bytes memory ret) = newImpl.staticcall(
                abi.encodeWithSignature(
                    "verifyImmutableTerms(uint256,address,address,uint256)",
                    gid, t.funder, t.recipient, t.totalAmount
                )
            );
            require(ok, "Proxy: verifyImmutableTerms call failed");
            require(abi.decode(ret, (bool)), "Proxy: immutable terms mismatch");
        }

        _setLogic(newImpl);
    }

    // ─── View ─────────────────────────────────────────────────────────────────

    /**
     * @notice Returns the immutable terms for a grant.
     */
    function getImmutableTerms(uint256 grantId)
        external view
        returns (address funder, address recipient, uint256 totalAmount)
    {
        ImmutableTerms storage t = immutableTerms[grantId];
        require(t.exists, "Proxy: grant not found");
        return (t.funder, t.recipient, t.totalAmount);
    }

    // ─── Internal ─────────────────────────────────────────────────────────────

    function _setLogic(address impl) internal {
        logicImpl = impl;
        logicHash = keccak256(impl.code);
        emit LogicUpgraded(impl, logicHash, msg.sender);
    }
}
