// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/**
 * @title IComplianceOfficer
 * @notice Interface for the Compliance Officer contract providing read-and-pause capabilities.
 */
interface IComplianceOfficer {
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

    // ─── View Functions ─────────────────────────────────────────────────────────

    function isGrantPaused(uint256 _grantId) external view returns (bool paused);
    function isAddressFlagged(address _address) external view returns (bool flagged);
    function getPauseInfo(uint256 _grantId) external view returns (PauseInfo memory pauseInfo);
    function getFlagInfo(address _address) external view returns (FlagInfo memory flagInfo);
    function getPausedGrants() external view returns (uint256[] memory);
    function getFlaggedAddresses() external view returns (address[] memory);
    function getComplianceOfficer() external view returns (address);

    // ─── Constants ────────────────────────────────────────────────────────────

    function MAX_PAUSE_DURATION() external view returns (uint256);
    function MIN_UNPAUSE_DELAY() external view returns (uint256);
}
