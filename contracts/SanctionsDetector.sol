// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/access/Ownable.sol";
import "./IComplianceOfficer.sol";

/**
 * @title SanctionsDetector
 * @notice Utility contract for detecting potential sanctions matches and automating compliance responses.
 * 
 * This contract provides a framework for:
 * - Maintaining a list of sanctioned addresses (can be updated by owner)
 * - Pattern detection for suspicious transaction patterns
 * - Automated flagging recommendations for the Compliance Officer
 * - Integration with external sanctions list oracles (future enhancement)
 * 
 * The detector operates independently but works closely with the Compliance Officer
 * to provide systematic enforcement of regulatory requirements.
 */
contract SanctionsDetector is Ownable {
    // ─── State ────────────────────────────────────────────────────────────────

    IComplianceOfficer public complianceOfficer;

    /// @notice Mapping of known sanctioned addresses
    mapping(address => bool) public sanctionedAddresses;

    /// @notice List of sanctioned addresses for enumeration
    address[] public sanctionedAddressList;

    /// @notice Mapping to track suspicious transaction patterns
    mapping(address => SuspiciousPattern) public suspiciousPatterns;

    /// @notice Configuration for automated detection
    DetectionConfig public config;

    // ─── Structs ──────────────────────────────────────────────────────────────

    struct SuspiciousPattern {
        uint256 transactionCount;
        uint256 totalVolume;
        uint256 firstSeen;
        uint256 lastActivity;
        bool flagged;
        uint256 patternType; // 1: high_frequency, 2: high_volume, 3: rapid_succession
    }

    struct DetectionConfig {
        uint256 maxTransactionRate;    // max transactions per hour
        uint256 maxVolumeThreshold;    // max volume before flagging (wei)
        uint256 minInterval;           // min time between transactions (seconds)
        bool autoFlagEnabled;          // enable automatic flagging
        uint256 autoFlagReason;        // reason code for auto-flagging
    }

    // ─── Events ───────────────────────────────────────────────────────────────

    event SanctionsDetectorUpdated(address indexed oldDetector, address indexed newDetector);
    event AddressAddedToSanctionsList(address indexed address, address indexed addedBy);
    event AddressRemovedFromSanctionsList(address indexed address, address indexed removedBy);
    event SuspiciousPatternDetected(address indexed address, uint256 patternType, string details);
    event AutoFlagAttempt(address indexed address, bool success, string reason);
    event DetectionConfigUpdated(uint256 maxRate, uint256 maxVolume, uint256 minInterval, bool autoFlag);

    // ─── Errors ───────────────────────────────────────────────────────────────

    error OnlyComplianceOfficer();
    error AddressAlreadySanctioned();
    error AddressNotSanctioned();
    error ComplianceOfficerNotSet();
    error InvalidConfig();

    // ─── Constructor ──────────────────────────────────────────────────────────

    constructor(address _complianceOfficer) Ownable(msg.sender) {
        if (_complianceOfficer != address(0)) {
            complianceOfficer = IComplianceOfficer(_complianceOfficer);
        }
        
        // Set default configuration
        config = DetectionConfig({
            maxTransactionRate: 100,        // 100 transactions per hour
            maxVolumeThreshold: 1000e18,    // 1000 ETH threshold
            minInterval: 60,                // 1 minute minimum
            autoFlagEnabled: false,         // Auto-flag disabled by default
            autoFlagReason: 2               // suspicious_activity
        });
    }

    // ─── Modifiers ────────────────────────────────────────────────────────────

    modifier onlyComplianceOfficer() {
        if (msg.sender != address(complianceOfficer)) revert OnlyComplianceOfficer();
        _;
    }

    // ─── Owner Functions ───────────────────────────────────────────────────────

    /**
     * @notice Update the Compliance Officer contract address.
     * @param _complianceOfficer New Compliance Officer contract address.
     */
    function setComplianceOfficer(address _complianceOfficer) external onlyOwner {
        address oldDetector = address(complianceOfficer);
        complianceOfficer = IComplianceOfficer(_complianceOfficer);
        emit SanctionsDetectorUpdated(oldDetector, _complianceOfficer);
    }

    /**
     * @notice Add an address to the sanctions list.
     * @param _address Address to add to sanctions list.
     */
    function addToSanctionsList(address _address) external onlyOwner {
        if (_address == address(0)) revert();
        if (sanctionedAddresses[_address]) revert AddressAlreadySanctioned();

        sanctionedAddresses[_address] = true;
        sanctionedAddressList.push(_address);
        emit AddressAddedToSanctionsList(_address, msg.sender);
    }

    /**
     * @notice Remove an address from the sanctions list.
     * @param _address Address to remove from sanctions list.
     */
    function removeFromSanctionsList(address _address) external onlyOwner {
        if (!sanctionedAddresses[_address]) revert AddressNotSanctioned();

        sanctionedAddresses[_address] = false;
        _removeFromSanctionedList(_address);
        emit AddressRemovedFromSanctionsList(_address, msg.sender);
    }

    /**
     * @notice Update detection configuration.
     * @param _maxTransactionRate Max transactions per hour.
     * @param _maxVolumeThreshold Max volume threshold in wei.
     * @param _minInterval Minimum time between transactions in seconds.
     * @param _autoFlagEnabled Enable automatic flagging.
     */
    function updateDetectionConfig(
        uint256 _maxTransactionRate,
        uint256 _maxVolumeThreshold,
        uint256 _minInterval,
        bool _autoFlagEnabled
    ) external onlyOwner {
        if (_maxTransactionRate == 0 || _maxVolumeThreshold == 0 || _minInterval == 0) {
            revert InvalidConfig();
        }

        config = DetectionConfig({
            maxTransactionRate: _maxTransactionRate,
            maxVolumeThreshold: _maxVolumeThreshold,
            minInterval: _minInterval,
            autoFlagEnabled: _autoFlagEnabled,
            autoFlagReason: 2 // suspicious_activity
        });

        emit DetectionConfigUpdated(_maxTransactionRate, _maxVolumeThreshold, _minInterval, _autoFlagEnabled);
    }

    // ─── Public Functions ───────────────────────────────────────────────────────

    /**
     * @notice Check if an address is on the sanctions list.
     * @param _address Address to check.
     * @return True if the address is sanctioned.
     */
    function isSanctioned(address _address) external view returns (bool) {
        return sanctionedAddresses[_address];
    }

    /**
     * @notice Analyze a transaction for suspicious patterns.
     * @param _address Address that performed the transaction.
     * @param _amount Transaction amount in wei.
     * @return suspicious True if suspicious pattern detected.
     * @return patternType Type of suspicious pattern detected.
     * @return details Human-readable details about the detection.
     */
    function analyzeTransaction(
        address _address,
        uint256 _amount
    ) external returns (bool suspicious, uint256 patternType, string memory details) {
        if (_address == address(0)) return (false, 0, "");

        SuspiciousPattern storage pattern = suspiciousPatterns[_address];
        uint256 currentTime = block.timestamp;

        // Initialize if first time seen
        if (pattern.firstSeen == 0) {
            pattern.firstSeen = currentTime;
        }

        // Update pattern data
        pattern.transactionCount++;
        pattern.totalVolume += _amount;
        pattern.lastActivity = currentTime;

        // Check for suspicious patterns
        (suspicious, patternType, details) = _detectSuspiciousPattern(_address, _amount, currentTime);

        if (suspicious) {
            pattern.flagged = true;
            pattern.patternType = patternType;
            emit SuspiciousPatternDetected(_address, patternType, details);

            // Attempt auto-flag if enabled
            if (config.autoFlagEnabled && address(complianceOfficer) != address(0)) {
                _attemptAutoFlag(_address, details);
            }
        }

        return (suspicious, patternType, details);
    }

    /**
     * @notice Get suspicious pattern information for an address.
     * @param _address Address to query.
     * @return pattern Complete pattern information.
     */
    function getSuspiciousPattern(address _address) external view returns (SuspiciousPattern memory pattern) {
        return suspiciousPatterns[_address];
    }

    /**
     * @notice Get all sanctioned addresses.
     * @return Array of sanctioned addresses.
     */
    function getSanctionedAddresses() external view returns (address[] memory) {
        return sanctionedAddressList;
    }

    /**
     * @notice Clear suspicious pattern data for an address (Compliance Officer only).
     * @param _address Address to clear.
     */
    function clearSuspiciousPattern(address _address) external onlyComplianceOfficer {
        delete suspiciousPatterns[_address];
    }

    // ─── Internal Functions ─────────────────────────────────────────────────────

    /**
     * @dev Detect suspicious patterns based on transaction analysis.
     * @param _address Address being analyzed.
     * @param _amount Transaction amount.
     * @param _currentTime Current timestamp.
     * @return suspicious True if pattern detected.
     * @return patternType Type of pattern.
     * @return details Description of the pattern.
     */
    function _detectSuspiciousPattern(
        address _address,
        uint256 _amount,
        uint256 _currentTime
    ) internal view returns (bool suspicious, uint256 patternType, string memory details) {
        SuspiciousPattern storage pattern = suspiciousPatterns[_address];

        // Check for high volume transactions
        if (_amount > config.maxVolumeThreshold) {
            return (true, 2, "High volume transaction detected");
        }

        // Check for rapid succession transactions
        if (pattern.lastActivity > 0 && (_currentTime - pattern.lastActivity) < config.minInterval) {
            return (true, 3, "Rapid succession transactions detected");
        }

        // Check for high frequency transactions (over last hour)
        uint256 hourAgo = _currentTime - 3600;
        uint256 recentCount = 0;
        
        // This is a simplified check - in production, you'd want more sophisticated tracking
        if (pattern.transactionCount > config.maxTransactionRate && pattern.firstSeen > hourAgo) {
            return (true, 1, "High frequency transactions detected");
        }

        return (false, 0, "");
    }

    /**
     * @dev Attempt to automatically flag an address with the Compliance Officer.
     * @param _address Address to flag.
     * @param _reason Reason for flagging.
     */
    function _attemptAutoFlag(address _address, string memory _reason) internal {
        try complianceOfficer.flagAddress(_address, config.autoFlagReason, _reason) {
            emit AutoFlagAttempt(_address, true, _reason);
        } catch {
            emit AutoFlagAttempt(_address, false, "Compliance Officer call failed");
        }
    }

    /**
     * @dev Remove an address from the sanctions list array.
     * @param _address Address to remove.
     */
    function _removeFromSanctionedList(address _address) internal {
        uint256 length = sanctionedAddressList.length;
        for (uint256 i = 0; i < length; i++) {
            if (sanctionedAddressList[i] == _address) {
                // Move the last element to the current position
                sanctionedAddressList[i] = sanctionedAddressList[length - 1];
                // Remove the last element
                sanctionedAddressList.pop();
                break;
            }
        }
    }
}
