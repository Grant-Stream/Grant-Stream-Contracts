// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import "./SustainabilityFund.sol";
import "./ArbitrationEscrow.sol";

/**
 * @title GrantStreamWithArbitration
 * @notice Enhanced GrantStream contract with integrated dispute resolution system
 * @dev Extends the original GrantStream functionality with arbitration capabilities
 *         When disputes are raised, funds are moved to escrow until resolved
 */
contract GrantStreamWithArbitration is Ownable, ReentrancyGuard {
    
    // ─── Constants ────────────────────────────────────────────────────────────
    
    uint256 public constant SUSTAINABILITY_TAX_BPS = 100;
    uint256 public constant BPS_DENOMINATOR = 1_000_000;
    uint256 public constant VOLUME_THRESHOLD = 100_000e18;
    
    // ─── Enums ────────────────────────────────────────────────────────────────
    
    enum GrantStatus { Active, InDispute, Disputed, Resolved, Closed }
    
    // ─── Structs ──────────────────────────────────────────────────────────────
    
    struct Grant {
        address funder;
        address recipient;
        uint256 balance;
        uint256 totalVolume;
        GrantStatus status;
        uint256 disputedAmount;     // Amount currently in dispute
        uint256 activeDisputeId;    // Current dispute ID if any
        bool exists;
    }
    
    // ─── State ────────────────────────────────────────────────────────────────
    
    SustainabilityFund public immutable sustainabilityFund;
    ArbitrationEscrow public arbitrationEscrow;
    
    uint256 public nextGrantId;
    mapping(uint256 => Grant) public grants;
    
    // ─── Events ───────────────────────────────────────────────────────────────
    
    event GrantCreated(uint256 indexed grantId, address indexed funder, address indexed recipient, uint256 amount);
    event FundsClaimed(uint256 indexed grantId, address indexed recipient, uint256 netAmount, uint256 sustainabilityTax);
    event GrantToppedUp(uint256 indexed grantId, uint256 amount);
    event GrantClosed(uint256 indexed grantId, uint256 refunded);
    event DisputeRaised(uint256 indexed grantId, uint256 indexed disputeId, address indexed funder, uint256 disputedAmount);
    event DisputeResolved(uint256 indexed grantId, uint256 indexed disputeId, GrantStatus newStatus);
    
    // ─── Constructor ──────────────────────────────────────────────────────────
    
    constructor(address _sustainabilityFund, address _arbitrationEscrow) Ownable(msg.sender) {
        require(_sustainabilityFund != address(0), "GrantStream: zero sustainability fund");
        require(_arbitrationEscrow != address(0), "GrantStream: zero arbitration escrow");
        sustainabilityFund = SustainabilityFund(payable(_sustainabilityFund));
        arbitrationEscrow = ArbitrationEscrow(_arbitrationEscrow);
    }
    
    // ─── External Functions ─────────────────────────────────────────────────────
    
    /**
     * @notice Create a new grant by depositing ETH
     * @param recipient Address that will receive streamed funds
     */
    function createGrant(address recipient) external payable nonReentrant returns (uint256 grantId) {
        require(msg.value > 0, "GrantStream: no funds");
        require(recipient != address(0), "GrantStream: zero recipient");
        
        grantId = nextGrantId++;
        grants[grantId] = Grant({
            funder: msg.sender,
            recipient: recipient,
            balance: msg.value,
            totalVolume: 0,
            status: GrantStatus.Active,
            disputedAmount: 0,
            activeDisputeId: 0,
            exists: true
        });
        
        emit GrantCreated(grantId, msg.sender, recipient, msg.value);
    }
    
    /**
     * @notice Recipient claims amount from their grant
     * @param grantId ID of the grant
     * @param amount Amount to claim
     */
    function claim(uint256 grantId, uint256 amount) external nonReentrant {
        Grant storage grant = grants[grantId];
        require(grant.exists, "GrantStream: grant does not exist");
        require(grant.status == GrantStatus.Active, "GrantStream: grant not active");
        require(msg.sender == grant.recipient, "GrantStream: not recipient");
        require(amount > 0 && amount <= grant.balance, "GrantStream: invalid amount");
        
        grant.balance -= amount;
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
        
        emit FundsClaimed(grantId, grant.recipient, net, tax);
    }
    
    /**
     * @notice Funder tops up an existing grant
     * @param grantId ID of the grant
     */
    function topUp(uint256 grantId) external payable nonReentrant {
        Grant storage grant = grants[grantId];
        require(grant.exists, "GrantStream: grant does not exist");
        require(grant.status == GrantStatus.Active, "GrantStream: grant not active");
        require(msg.sender == grant.funder, "GrantStream: not funder");
        require(msg.value > 0, "GrantStream: no funds");
        
        grant.balance += msg.value;
        emit GrantToppedUp(grantId, msg.value);
    }
    
    /**
     * @notice Funder raises a dispute on a grant
     * @param grantId ID of the grant to dispute
     * @param disputedAmount Amount being disputed
     * @param evidence IPFS hash or URL of evidence
     * @param reason Reason for the dispute
     */
    function raiseDispute(
        uint256 grantId,
        uint256 disputedAmount,
        string memory evidence,
        string memory reason
    ) external nonReentrant {
        Grant storage grant = grants[grantId];
        require(grant.exists, "GrantStream: grant does not exist");
        require(grant.status == GrantStatus.Active, "GrantStream: grant not active");
        require(msg.sender == grant.funder, "GrantStream: not funder");
        require(disputedAmount > 0 && disputedAmount <= grant.balance, "GrantStream: invalid dispute amount");
        require(grant.activeDisputeId == 0, "GrantStream: dispute already active");
        
        // Move funds to escrow and update grant status
        grant.balance -= disputedAmount;
        grant.disputedAmount = disputedAmount;
        grant.status = GrantStatus.InDispute;
        
        // Create dispute in arbitration escrow
        uint256 disputeId = arbitrationEscrow.raiseDispute{value: disputedAmount}(
            grantId,
            grant.funder,
            grant.recipient,
            disputedAmount,
            evidence,
            reason
        );
        
        grant.activeDisputeId = disputeId;
        
        emit DisputeRaised(grantId, disputeId, grant.funder, disputedAmount);
    }
    
    /**
     * @notice Called by ArbitrationEscrow when a dispute is resolved
     * @param grantId ID of the grant
     * @param disputeId ID of the resolved dispute
     * @param funderAward Amount awarded to funder
     * @param granteeAward Amount awarded to grantee
     */
    function onDisputeResolved(
        uint256 grantId,
        uint256 disputeId,
        uint256 funderAward,
        uint256 granteeAward
    ) external nonReentrant {
        require(msg.sender == address(arbitrationEscrow), "GrantStream: only arbitration escrow");
        Grant storage grant = grants[grantId];
        require(grant.exists, "GrantStream: grant does not exist");
        require(grant.activeDisputeId == disputeId, "GrantStream: mismatched dispute");
        require(grant.status == GrantStatus.InDispute, "GrantStream: not in dispute");
        
        // Update grant status and reset dispute tracking
        grant.status = GrantStatus.Resolved;
        grant.disputedAmount = 0;
        grant.activeDisputeId = 0;
        
        // Return any remaining disputed funds back to grant balance
        uint256 totalAwarded = funderAward + granteeAward;
        uint256 remainingDispute = grant.disputedAmount - totalAwarded;
        if (remainingDispute > 0) {
            grant.balance += remainingDispute;
        }
        
        emit DisputeResolved(grantId, disputeId, grant.status);
    }
    
    /**
     * @notice Funder closes the grant and reclaims remaining balance
     * @param grantId ID of the grant to close
     */
    function closeGrant(uint256 grantId) external nonReentrant {
        Grant storage grant = grants[grantId];
        require(grant.exists, "GrantStream: grant does not exist");
        require(grant.status == GrantStatus.Active || grant.status == GrantStatus.Resolved, "GrantStream: cannot close");
        require(msg.sender == grant.funder, "GrantStream: not funder");
        require(grant.activeDisputeId == 0, "GrantStream: active dispute");
        
        grant.status = GrantStatus.Closed;
        uint256 refund = grant.balance;
        grant.balance = 0;
        
        if (refund > 0) {
            (bool ok, ) = grant.funder.call{value: refund}("");
            require(ok, "GrantStream: refund failed");
        }
        
        emit GrantClosed(grantId, refund);
    }
    
    /**
     * @notice Get grant details
     * @param grantId ID of the grant
     * @return Complete grant information
     */
    function getGrant(uint256 grantId) external view returns (Grant memory) {
        return grants[grantId];
    }
    
    /**
     * @notice Check if a grant has an active dispute
     * @param grantId ID of the grant
     * @return Whether the grant has an active dispute
     */
    function hasActiveDispute(uint256 grantId) external view returns (bool) {
        Grant memory grant = grants[grantId];
        return grant.exists && grant.activeDisputeId != 0;
    }
    
    /**
     * @notice Get the active dispute ID for a grant
     * @param grantId ID of the grant
     * @return Active dispute ID (0 if none)
     */
    function getActiveDisputeId(uint256 grantId) external view returns (uint256) {
        Grant memory grant = grants[grantId];
        return grant.exists ? grant.activeDisputeId : 0;
    }

    // ─── Wasm-Rotation Hook ───────────────────────────────────────────────────

    /**
     * @notice Proxy spot-check: confirms this logic version agrees with the
     *         immutable terms stored in GrantStreamProxy for a given grant.
     */
    function verifyImmutableTerms(
        uint256 grantId,
        address funder,
        address recipient,
        uint256 totalAmount
    ) external view returns (bool) {
        Grant storage g = grants[grantId];
        if (!g.exists) return false;
        return g.funder    == funder    &&
               g.recipient == recipient &&
               (g.balance + g.totalVolume) == totalAmount;
    }
    
    // ─── Internal Functions ─────────────────────────────────────────────────────
    
    function _computeSustainabilityTax(
        uint256 totalVolumeAfter,
        uint256 claimAmount
    ) internal pure returns (uint256 tax) {
        if (totalVolumeAfter <= VOLUME_THRESHOLD) {
            return 0;
        }
        
        uint256 totalVolumeBefore = totalVolumeAfter - claimAmount;
        
        uint256 taxableAmount;
        if (totalVolumeBefore >= VOLUME_THRESHOLD) {
            taxableAmount = claimAmount;
        } else {
            taxableAmount = totalVolumeAfter - VOLUME_THRESHOLD;
        }
        
        tax = (taxableAmount * SUSTAINABILITY_TAX_BPS) / BPS_DENOMINATOR;
    }
}
