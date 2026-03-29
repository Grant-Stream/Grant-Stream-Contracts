// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import "./IComplianceOfficer.sol";

/**
 * @title ComplianceOfficer
 * @notice On-Chain Compliance Officer role with "Read-and-Pause" rights for regulatory oversight.
 * 
 * This contract implements a restricted Auditor role that can:
 * - Flag transactions for review when sanctions matches are detected
 * - Temporarily pause grant streams to prevent fund flow to sanctioned addresses
 * - Read all grant and transaction data for monitoring purposes
 * - CANNOT redirect funds or modify grant parameters (separation of powers)
 * 
 * The Compliance Officer operates independently from the core DAO while providing
 * the human-in-the-loop oversight required by regulated institutions.
 */
contract ComplianceOfficer is IComplianceOfficer, Ownable, ReentrancyGuard {
    // ─── Constants ────────────────────────────────────────────────────────────

    uint256 public constant MAX_PAUSE_DURATION = 30 days;
    uint256 public constant MIN_UNPAUSE_DELAY = 1 hours;

    // ─── State ────────────────────────────────────────────────────────────────

    /// @notice Address of the current Compliance Officer (can be updated by owner)
    address public complianceOfficer;

    /// @notice Mapping of paused grant IDs to pause metadata
    mapping(uint256 => PauseInfo) public pausedGrants;

    /// @notice Mapping of flagged addresses with metadata
    mapping(address => FlagInfo) public flaggedAddresses;

    /// @notice List of all currently paused grants for enumeration
    uint256[] public pausedGrantIds;

    /// @notice List of all flagged addresses for enumeration
    address[] public flaggedAddressList;

    /// @notice Total number of compliance actions taken
    uint256 public totalActions;

    // ─── Structs ──────────────────────────────────────────────────────────────

    struct PauseInfo {
        bool paused;
        address pausedBy;
        uint256 pausedAt;
        uint256 pauseReason; // 1: sanctions_match, 2: suspicious_activity, 3: regulatory_review
        string reasonDetails;
        uint256 earliestUnpause;
    }

    struct FlagInfo {
        bool flagged;
        address flaggedBy;
        uint256 flaggedAt;
        uint256 flagReason; // 1: sanctions_match, 2: suspicious_activity, 3: regulatory_review
        string reasonDetails;
        bool active;
    }

    // ─── Events ───────────────────────────────────────────────────────────────

    event ComplianceOfficerUpdated(address indexed oldOfficer, address indexed newOfficer);
    event GrantPaused(uint256 indexed grantId, address indexed pausedBy, uint256 reason, string details);
    event GrantUnpaused(uint256 indexed grantId, address indexed unpausedBy);
    event AddressFlagged(address indexed flaggedAddress, address indexed flaggedBy, uint256 reason, string details);
    event AddressUnflagged(address indexed flaggedAddress, address indexed unflaggedBy);
    event ComplianceAction(uint256 indexed actionId, string action, address indexed target);

    // ─── Errors ───────────────────────────────────────────────────────────────

    error OnlyComplianceOfficer();
    error OnlyOwnerOrComplianceOfficer();
    error GrantNotPaused();
    error GrantAlreadyPaused();
    error AddressNotFlagged();
    error AddressAlreadyFlagged();
    error InvalidPauseReason();
    error InvalidFlagReason();
    error PauseDurationExceeded();
    error UnpauseTooEarly();
    error ZeroAddress();

    // ─── Constructor ──────────────────────────────────────────────────────────

    constructor(address _complianceOfficer) Ownable(msg.sender) {
        if (_complianceOfficer == address(0)) revert ZeroAddress();
        complianceOfficer = _complianceOfficer;
        emit ComplianceOfficerUpdated(address(0), _complianceOfficer);
    }

    // ─── Modifiers ────────────────────────────────────────────────────────────

    modifier onlyComplianceOfficer() {
        if (msg.sender != complianceOfficer) revert OnlyComplianceOfficer();
        _;
    }

    modifier onlyOwnerOrComplianceOfficer() {
        if (msg.sender != owner() && msg.sender != complianceOfficer) {
            revert OnlyOwnerOrComplianceOfficer();
        }
        _;
    }

    modifier validPauseReason(uint256 _reason) {
        if (_reason < 1 || _reason > 3) revert InvalidPauseReason();
        _;
    }

    modifier validFlagReason(uint256 _reason) {
        if (_reason < 1 || _reason > 3) revert InvalidFlagReason();
        _;
    }

    // ─── Owner Functions ───────────────────────────────────────────────────────

    /**
     * @notice Update the Compliance Officer address (owner only).
     * @param _newOfficer New Compliance Officer address.
     */
    function setComplianceOfficer(address _newOfficer) external onlyOwner {
        if (_newOfficer == address(0)) revert ZeroAddress();
        address oldOfficer = complianceOfficer;
        complianceOfficer = _newOfficer;
        emit ComplianceOfficerUpdated(oldOfficer, _newOfficer);
    }

    // ─── Compliance Officer Functions ───────────────────────────────────────────

    /**
     * @notice Pause a grant stream due to sanctions match or regulatory concerns.
     * @param _grantId Grant ID to pause.
     * @param _reason Reason for pausing (1=sanctions_match, 2=suspicious_activity, 3=regulatory_review).
     * @param _details Human-readable details about the pause reason.
     */
    function pauseGrant(
        uint256 _grantId,
        uint256 _reason,
        string calldata _details
    ) external onlyComplianceOfficer validPauseReason(_reason) nonReentrant {
        if (pausedGrants[_grantId].paused) revert GrantAlreadyPaused();

        pausedGrants[_grantId] = PauseInfo({
            paused: true,
            pausedBy: msg.sender,
            pausedAt: block.timestamp,
            pauseReason: _reason,
            reasonDetails: _details,
            earliestUnpause: block.timestamp + MIN_UNPAUSE_DELAY
        });

        pausedGrantIds.push(_grantId);
        totalActions++;

        emit GrantPaused(_grantId, msg.sender, _reason, _details);
        emit ComplianceAction(totalActions, "pause_grant", address(uint160(_grantId)));
    }

    /**
     * @notice Unpause a previously paused grant.
     * @param _grantId Grant ID to unpause.
     */
    function unpauseGrant(uint256 _grantId) external onlyComplianceOfficer nonReentrant {
        PauseInfo storage pauseInfo = pausedGrants[_grantId];
        if (!pauseInfo.paused) revert GrantNotPaused();
        if (block.timestamp < pauseInfo.earliestUnpause) revert UnpauseTooEarly();

        // Remove from paused grants array
        _removeFromPausedGrants(_grantId);

        // Clear pause info
        delete pausedGrants[_grantId];
        totalActions++;

        emit GrantUnpaused(_grantId, msg.sender);
        emit ComplianceAction(totalActions, "unpause_grant", address(uint160(_grantId)));
    }

    /**
     * @notice Flag an address for compliance monitoring.
     * @param _address Address to flag.
     * @param _reason Reason for flagging (1=sanctions_match, 2=suspicious_activity, 3=regulatory_review).
     * @param _details Human-readable details about the flag reason.
     */
    function flagAddress(
        address _address,
        uint256 _reason,
        string calldata _details
    ) external onlyComplianceOfficer validFlagReason(_reason) nonReentrant {
        if (_address == address(0)) revert ZeroAddress();
        if (flaggedAddresses[_address].flagged && flaggedAddresses[_address].active) {
            revert AddressAlreadyFlagged();
        }

        flaggedAddresses[_address] = FlagInfo({
            flagged: true,
            flaggedBy: msg.sender,
            flaggedAt: block.timestamp,
            flagReason: _reason,
            reasonDetails: _details,
            active: true
        });

        // Add to flagged addresses list if not already present
        if (!flaggedAddresses[_address].flagged) {
            flaggedAddressList.push(_address);
        }

        totalActions++;

        emit AddressFlagged(_address, msg.sender, _reason, _details);
        emit ComplianceAction(totalActions, "flag_address", _address);
    }

    /**
     * @notice Remove a flag from an address.
     * @param _address Address to unflag.
     */
    function unflagAddress(address _address) external onlyComplianceOfficer nonReentrant {
        if (!flaggedAddresses[_address].flagged || !flaggedAddresses[_address].active) {
            revert AddressNotFlagged();
        }

        flaggedAddresses[_address].active = false;
        totalActions++;

        emit AddressUnflagged(_address, msg.sender);
        emit ComplianceAction(totalActions, "unflag_address", _address);
    }

    // ─── View Functions ─────────────────────────────────────────────────────────

    /**
     * @notice Check if a grant is currently paused.
     * @param _grantId Grant ID to check.
     * @return paused True if the grant is paused.
     */
    function isGrantPaused(uint256 _grantId) external view returns (bool paused) {
        return pausedGrants[_grantId].paused;
    }

    /**
     * @notice Check if an address is flagged.
     * @param _address Address to check.
     * @return flagged True if the address is flagged and active.
     */
    function isAddressFlagged(address _address) external view returns (bool flagged) {
        return flaggedAddresses[_address].flagged && flaggedAddresses[_address].active;
    }

    /**
     * @notice Get pause information for a grant.
     * @param _grantId Grant ID to query.
     * @return pauseInfo Complete pause information.
     */
    function getPauseInfo(uint256 _grantId) external view returns (PauseInfo memory pauseInfo) {
        return pausedGrants[_grantId];
    }

    /**
     * @notice Get flag information for an address.
     * @param _address Address to query.
     * @return flagInfo Complete flag information.
     */
    function getFlagInfo(address _address) external view returns (FlagInfo memory flagInfo) {
        return flaggedAddresses[_address];
    }

    /**
     * @notice Get all currently paused grant IDs.
     * @return Array of paused grant IDs.
     */
    function getPausedGrants() external view returns (uint256[] memory) {
        return pausedGrantIds;
    }

    /**
     * @notice Get all flagged addresses.
     * @return Array of flagged addresses.
     */
    function getFlaggedAddresses() external view returns (address[] memory) {
        return flaggedAddressList;
    }

    /**
     * @notice Get the current Compliance Officer address.
     * @return Address of the current Compliance Officer.
     */
    function getComplianceOfficer() external view returns (address) {
        return complianceOfficer;
    }

    // ─── Internal Functions ─────────────────────────────────────────────────────

    /**
     * @dev Remove a grant ID from the paused grants array.
     * @param _grantId Grant ID to remove.
     */
    function _removeFromPausedGrants(uint256 _grantId) internal {
        uint256 length = pausedGrantIds.length;
        for (uint256 i = 0; i < length; i++) {
            if (pausedGrantIds[i] == _grantId) {
                // Move the last element to the current position
                pausedGrantIds[i] = pausedGrantIds[length - 1];
                // Remove the last element
                pausedGrantIds.pop();
                break;
            }
        }
    }
}
