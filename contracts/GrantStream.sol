// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import "./SustainabilityFund.sol";

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

    struct Grant {
        address funder;
        address recipient;
        uint256 balance;          // remaining claimable balance
        uint256 totalVolume;      // cumulative amount ever streamed / claimed
        bool    active;
    }

    uint256 public nextGrantId;
    mapping(uint256 => Grant) public grants;

    // ─── Events ───────────────────────────────────────────────────────────────

    event GrantCreated(uint256 indexed grantId, address indexed funder, address indexed recipient, uint256 amount);
    event FundsClaimed(uint256 indexed grantId, address indexed recipient, uint256 netAmount, uint256 sustainabilityTax);
    event GrantToppedUp(uint256 indexed grantId, uint256 amount);
    event GrantClosed(uint256 indexed grantId, uint256 refunded);

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
    function createGrant(address recipient) external payable nonReentrant returns (uint256 grantId) {
        require(msg.value > 0, "GrantStream: no funds");
        require(recipient != address(0), "GrantStream: zero recipient");

        grantId = nextGrantId++;
        grants[grantId] = Grant({
            funder:      msg.sender,
            recipient:   recipient,
            balance:     msg.value,
            totalVolume: 0,
            active:      true
        });

        emit GrantCreated(grantId, msg.sender, recipient, msg.value);
    }

    /**
     * @notice Recipient claims `amount` from their grant.
     *         Applies the 0.01% sustainability tax when cumulative volume >= VOLUME_THRESHOLD.
     */
    function claim(uint256 grantId, uint256 amount) external nonReentrant {
        Grant storage grant = grants[grantId];
        require(grant.active, "GrantStream: inactive grant");
        require(msg.sender == grant.recipient, "GrantStream: not recipient");
        require(amount > 0 && amount <= grant.balance, "GrantStream: invalid amount");

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
}
