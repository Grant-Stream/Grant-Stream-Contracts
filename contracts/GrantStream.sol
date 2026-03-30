// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import "./SustainabilityFund.sol";
import "./ZKKYCVerifier.sol";

/**
 * @title GrantStream
 * @notice Core grant streaming contract. Funders deposit ETH into grants,
 *         recipients stream funds over time. Once a grant's cumulative volume
 *         crosses $100,000 (represented as 100_000e18 in token units), the
 *         Final_Protocol_Sustainability_Fund_Transfer logic activates and
 *         redirects 0.01% of each subsequent transfer to the
 *         JerryIdoko Developer Treasury via SustainabilityFund.
 */
contract GrantStream is Ownable, ReentrancyGuard {
    // ─── Constants ────────────────────────────────────────────────────────────

    /// @dev 0.01% expressed as basis points out of 1_000_000 (i.e. 100 = 0.01%)
    uint256 public constant SUSTAINABILITY_TAX_BPS = 100;
    uint256 public constant BPS_DENOMINATOR = 1_000_000;

    /// @dev Volume threshold (in wei) above which the sustainability tax kicks in.
    ///      Represents $100,000 worth of the native token at protocol-defined parity.
    ///      For ERC-20 integrations this should be overridden per token decimals.
    uint256 public constant VOLUME_THRESHOLD = 100_000e18;

    // ─── State ────────────────────────────────────────────────────────────────

    SustainabilityFund public immutable sustainabilityFund;

    /// @notice Optional ZK-KYC verifier. address(0) = KYC checks disabled.
    ZKKYCVerifier public zkVerifier;

    /// @notice When true, both the recipient at grant creation and the claimer
    ///         at claim time must be verified in zkVerifier.
    bool public kycRequired;

    struct Grant {
        address funder;
        address recipient;
        uint256 balance;          // remaining claimable balance
        uint256 totalVolume;      // cumulative amount ever streamed / claimed
        bool    active;
        bool    finalReleaseRequired;  // Flag: last 10% requires community approval
        bool    finalReleaseApproved;  // Flag: community has approved final release
        uint256 endDate;          // Grant stream end date
        bool    exists;           // Flag to track if grant exists
    }

    uint256 public nextGrantId;
    mapping(uint256 => Grant) public grants;

    // ─── Events ───────────────────────────────────────────────────────────────

    event GrantCreated(uint256 indexed grantId, address indexed funder, address indexed recipient, uint256 amount);
    event FundsClaimed(uint256 indexed grantId, address indexed recipient, uint256 netAmount, uint256 sustainabilityTax);
    event GrantToppedUp(uint256 indexed grantId, uint256 amount);
    event GrantClosed(uint256 indexed grantId, uint256 refunded);
    event ZKVerifierSet(address indexed zkVerifier);
    event KYCRequirementChanged(bool required);
    event FinalReleaseFlagSet(uint256 indexed grantId, bool required);
    event FinalReleaseApproved(uint256 indexed grantId, address indexed approver, uint256 timestamp);
    event FinalReleaseClaimed(uint256 indexed grantId, address indexed recipient, uint256 amount);

    // ─── Constructor ──────────────────────────────────────────────────────────

    constructor(address _sustainabilityFund) Ownable(msg.sender) {
        require(_sustainabilityFund != address(0), "GrantStream: zero address");
        sustainabilityFund = SustainabilityFund(payable(_sustainabilityFund));
    }

    // ─── External ─────────────────────────────────────────────────────────────

    /**
     * @notice Create a new grant by depositing ETH.
     * @param recipient Address that will receive streamed funds.
     */
    /**
     * @notice Owner sets or clears the ZK-KYC verifier contract.
     * @param _zkVerifier Address of ZKKYCVerifier, or address(0) to disable.
     */
    function setZKVerifier(address _zkVerifier) external onlyOwner {
        zkVerifier = ZKKYCVerifier(_zkVerifier);
        emit ZKVerifierSet(_zkVerifier);
    }

    /**
     * @notice Owner toggles whether KYC verification is required for grants.
     *         Requires zkVerifier to be set before enabling.
     * @param _required True to enforce KYC; false to allow permissionless grants.
     */
    function setKYCRequired(bool _required) external onlyOwner {
        if (_required) {
            require(address(zkVerifier) != address(0), "GrantStream: zkVerifier not set");
        }
        kycRequired = _required;
        emit KYCRequirementChanged(_required);
    }

    /**
     * @notice Create a new grant by depositing ETH.
     * @param recipient Address that will receive streamed funds.
     * @param _endDate Timestamp when the grant stream ends (0 for no end date).
     * @param _finalReleaseRequired Whether the last 10% requires community approval.
     */
    function createGrant(
        address recipient, 
        uint256 _endDate,
        bool _finalReleaseRequired
    ) external payable nonReentrant returns (uint256 grantId) {
        require(msg.value > 0, "GrantStream: no funds");
        require(recipient != address(0), "GrantStream: zero recipient");
        if (kycRequired) {
            require(zkVerifier.isVerified(recipient), "GrantStream: recipient not KYC verified");
        }

        grantId = nextGrantId++;
        grants[grantId] = Grant({
            funder:               msg.sender,
            recipient:            recipient,
            balance:              msg.value,
            totalVolume:          0,
            active:               true,
            finalReleaseRequired: _finalReleaseRequired,
            finalReleaseApproved: false,
            endDate:              _endDate,
            exists:               true
        });

        emit GrantCreated(grantId, msg.sender, recipient, msg.value);
        if (_finalReleaseRequired) {
            emit FinalReleaseFlagSet(grantId, true);
        }
    }

    /**
     * @notice Backward-compatible createGrant without final release parameters.
     * @param recipient Address that will receive streamed funds.
     */
    function createGrant(address recipient) external payable nonReentrant returns (uint256 grantId) {
        return createGrant(recipient, 0, false);
    }

    /**
     * @notice Recipient claims `amount` from their grant.
     *         Applies the 0.01% sustainability tax when cumulative volume >= VOLUME_THRESHOLD.
     *         If finalReleaseRequired is enabled and grant has ended, last 10% requires community approval.
     */
    function claim(uint256 grantId, uint256 amount) external nonReentrant {
        Grant storage grant = grants[grantId];
        require(grant.active, "GrantStream: inactive grant");
        require(msg.sender == grant.recipient, "GrantStream: not recipient");
        require(amount > 0 && amount <= grant.balance, "GrantStream: invalid amount");
        if (kycRequired) {
            require(zkVerifier.isVerified(msg.sender), "GrantStream: recipient not KYC verified");
        }

        // Check if this is the final 10% and requires community handshake
        uint256 remainingBalance = grant.balance;
        uint256 tenPercentOfOriginal = (grant.totalVolume + remainingBalance) / 10;
        
        // If final release is required, grant has ended, and this is the last 10%
        if (grant.finalReleaseRequired && 
            grant.endDate > 0 && 
            block.timestamp > grant.endDate &&
            amount <= tenPercentOfOriginal &&
            !grant.finalReleaseApproved) {
            revert("GrantStream: Last 10% requires community approval vote");
        }

        grant.balance     -= amount;
        grant.totalVolume += amount;

        uint256 tax = _computeSustainabilityTax(grant.totalVolume, amount);
        uint256 net = amount - tax;

        // Transfer sustainability tax to the fund
        if (tax > 0) {
            sustainabilityFund.deposit{value: tax}();
        }

        // Transfer net amount to recipient
        (bool ok, ) = grant.recipient.call{value: net}("");
        require(ok, "GrantStream: transfer failed");

        // Check if this was the final release
        if (grant.finalReleaseRequired && grant.balance == 0) {
            emit FinalReleaseClaimed(grantId, grant.recipient, amount);
        }

        emit FundsClaimed(grantId, grant.recipient, net, tax);
    }

    /**
     * @notice Funder tops up an existing grant.
     */
    function topUp(uint256 grantId) external payable nonReentrant {
        Grant storage grant = grants[grantId];
        require(grant.active, "GrantStream: inactive grant");
        require(msg.sender == grant.funder, "GrantStream: not funder");
        require(msg.value > 0, "GrantStream: no funds");

        grant.balance += msg.value;
        emit GrantToppedUp(grantId, msg.value);
    }

    /**
     * @notice Community governance approves the final release for grants with finalReleaseRequired flag.
     *         This allows the last 10% to be claimed after a successful project launch vote.
     * @param grantId ID of the grant to approve.
     */
    function approveFinalRelease(uint256 grantId) external nonReentrant {
        Grant storage grant = grants[grantId];
        require(grant.finalReleaseRequired, "GrantStream: Final release not required for this grant");
        require(!grant.finalReleaseApproved, "GrantStream: Final release already approved");
        require(grant.endDate > 0 && block.timestamp > grant.endDate, 
                "GrantStream: Grant has not ended yet");
        
        // In a full implementation, this would check DAO voting power
        // For now, we use a simple owner-based approval as placeholder
        // A real implementation would integrate with a DAO governance contract
        require(msg.sender == owner(), "GrantStream: Only owner/governance can approve final release");
        
        grant.finalReleaseApproved = true;
        emit FinalReleaseApproved(grantId, msg.sender, block.timestamp);
    }

    /**
     * @notice Funder closes the grant and reclaims remaining balance.
     */
    function closeGrant(uint256 grantId) external nonReentrant {
        Grant storage grant = grants[grantId];
        require(grant.active, "GrantStream: inactive grant");
        require(msg.sender == grant.funder, "GrantStream: not funder");

        grant.active = false;
        uint256 refund = grant.balance;
        grant.balance = 0;

        if (refund > 0) {
            (bool ok, ) = grant.funder.call{value: refund}("");
            require(ok, "GrantStream: refund failed");
        }

        emit GrantClosed(grantId, refund);
    }

    // ─── Internal ─────────────────────────────────────────────────────────────

    /**
     * @dev Computes the sustainability tax for a claim.
     *      Tax is only applied once the grant's cumulative volume has crossed
     *      VOLUME_THRESHOLD (i.e. $100,000+).
     *
     *      If the claim itself straddles the threshold, only the portion above
     *      the threshold is taxed, keeping small builders completely free.
     *
     * @param totalVolumeAfter  Grant's totalVolume AFTER adding this claim.
     * @param claimAmount       The raw amount being claimed.
     */
    function _computeSustainabilityTax(
        uint256 totalVolumeAfter,
        uint256 claimAmount
    ) internal pure returns (uint256 tax) {
        if (totalVolumeAfter <= VOLUME_THRESHOLD) {
            // Entire claim is below threshold — no tax
            return 0;
        }

        uint256 totalVolumeBefore = totalVolumeAfter - claimAmount;

        uint256 taxableAmount;
        if (totalVolumeBefore >= VOLUME_THRESHOLD) {
            // Entire claim is above threshold
            taxableAmount = claimAmount;
        } else {
            // Claim straddles the threshold — only tax the portion above it
            taxableAmount = totalVolumeAfter - VOLUME_THRESHOLD;
        }

        tax = (taxableAmount * SUSTAINABILITY_TAX_BPS) / BPS_DENOMINATOR;
    }

    /**
     * @notice Get detailed grant information including final release status.
     * @param grantId ID of the grant.
     * @return Grant details with final release flags.
     */
    function getGrantDetails(uint256 grantId) external view returns (Grant memory) {
        require(grants[grantId].exists || grantId < nextGrantId, "GrantStream: Grant does not exist");
        return grants[grantId];
    }

    /**
     * @notice Check if a grant requires final community approval for the last 10%.
     * @param grantId ID of the grant.
     * @return True if final release is required and not yet approved.
     */
    function requiresFinalApproval(uint256 grantId) external view returns (bool) {
        Grant storage grant = grants[grantId];
        return grant.finalReleaseRequired && 
               !grant.finalReleaseApproved && 
               grant.endDate > 0 && 
               block.timestamp > grant.endDate;
    }
}
