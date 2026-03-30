// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import "./GrantStream.sol";

/**
 * @title GrantConsolidator
 * @notice "Management Vault" for professional teams to consolidate multiple grant streams.
 *         Maintains individual accounting for each DAO grantor while providing
 *         a unified management experience.
 *
 *         This "Professional Dashboard" experience makes it easy for lead developers 
 *         to manage their entire project budget in one place, reducing the chance 
 *         of accounting errors or missed payroll for their sub-teams.
 */
contract GrantConsolidator is Ownable, ReentrancyGuard {
    // ─── State ────────────────────────────────────────────────────────────────

    GrantStream public immutable grantStream;

    // Mapping from grantId to accounting data
    struct GrantAccounting {
        uint256 originalGrantId;
        address grantor; // funder of the grant
        uint256 totalClaimed;
        bool exists;
    }

    uint256[] public consolidatedGrants;
    mapping(uint256 => GrantAccounting) public grantDetails;
    
    // Total amount claimed per grantor (DAO)
    mapping(address => uint256) public totalReceivedPerGrantor;

    // ─── Events ───────────────────────────────────────────────────────────────

    event GrantAdded(uint256 indexed grantId, address indexed grantor);
    event FundsConsolidated(uint256 indexed grantId, address indexed grantor, uint256 amount);
    event Withdrawal(address indexed to, uint256 amount);

    // ─── Constructor ──────────────────────────────────────────────────────────

    /**
     * @param _grantStream The core GrantStream contract address.
     * @param _owner The lead developer or team treasury address.
     */
    constructor(address _grantStream, address _owner) Ownable(_owner) {
        require(_grantStream != address(0), "GrantConsolidator: zero grantStream");
        grantStream = GrantStream(_grantStream);
    }

    // ─── External ─────────────────────────────────────────────────────────────

    /**
     * @notice Registers a grant with this consolidator.
     *         The consolidator must already be the recipient of the grant in GrantStream.
     */
    function addGrant(uint256 grantId) external onlyOwner {
        require(!grantDetails[grantId].exists, "GrantConsolidator: grant already added");
        
        // Fetch grant info from GrantStream
        (address funder, address recipient, , , bool active, , , , ) = grantStream.grants(grantId);
        
        require(active, "GrantConsolidator: grant not active");
        require(recipient == address(this), "GrantConsolidator: consolidator not recipient");

        grantDetails[grantId] = GrantAccounting({
            originalGrantId: grantId,
            grantor: funder,
            totalClaimed: 0,
            exists: true
        });

        consolidatedGrants.push(grantId);
        emit GrantAdded(grantId, funder);
    }

    /**
     * @notice Claims funds from a specific grant and updates local accounting.
     */
    function claimFromGrant(uint256 grantId, uint256 amount) external onlyOwner nonReentrant {
        _claim(grantId, amount);
    }

    /**
     * @notice Claims from multiple grants in one transaction.
     */
    function batchClaim(uint256[] calldata grantIds, uint256[] calldata amounts) external onlyOwner nonReentrant {
        require(grantIds.length == amounts.length, "GrantConsolidator: length mismatch");
        
        for (uint256 i = 0; i < grantIds.length; i++) {
            _claim(grantIds[i], amounts[i]);
        }
    }

    /**
     * @notice Withdraw consolidated funds to the team treasury or payroll.
     * @param to Recipient of the funds.
     * @param amount Amount to withdraw (0 = all).
     */
    function withdraw(address payable to, uint256 amount) external onlyOwner nonReentrant {
        require(to != address(0), "GrantConsolidator: zero address");
        uint256 available = address(this).balance;
        uint256 toSend = amount == 0 ? available : amount;
        require(toSend <= available, "GrantConsolidator: insufficient balance");

        (bool ok, ) = to.call{value: toSend}("");
        require(ok, "GrantConsolidator: withdrawal failed");

        emit Withdrawal(to, toSend);
    }

    // ─── View ─────────────────────────────────────────────────────────────────

    /**
     * @notice Unified view for the professional dashboard.
     */
    function getVaultSummary() external view returns (
        uint256 totalVaultBalance,
        uint256 numberOfGrants,
        uint256 totalClaimedHistory
    ) {
        totalVaultBalance = address(this).balance;
        numberOfGrants = consolidatedGrants.length;
        
        for (uint256 i = 0; i < consolidatedGrants.length; i++) {
            totalClaimedHistory += grantDetails[consolidatedGrants[i]].totalClaimed;
        }
    }

    /**
     * @notice Get all registered grant IDs in this vault.
     */
    function getConsolidatedGrants() external view returns (uint256[] memory) {
        return consolidatedGrants;
    }

    // ─── Internal ─────────────────────────────────────────────────────────────

    function _claim(uint256 grantId, uint256 amount) internal {
        require(grantDetails[grantId].exists, "GrantConsolidator: grant not added");

        uint256 balanceBefore = address(this).balance;
        
        // This will call GrantStream and trigger the ETH transfer back to this contract
        grantStream.claim(grantId, amount);
        
        uint256 received = address(this).balance - balanceBefore;
        if (received > 0) {
            grantDetails[grantId].totalClaimed += received;
            totalReceivedPerGrantor[grantDetails[grantId].grantor] += received;
            emit FundsConsolidated(grantId, grantDetails[grantId].grantor, received);
        }
    }

    /**
     * @notice Allows receiving ETH from GrantStream.
     */
    receive() external payable {}
}
