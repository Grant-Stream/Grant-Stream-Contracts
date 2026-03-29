// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
import "@openzeppelin/contracts/utils/Counters.sol";
import "./libraries/HashUtils.sol";

/**
 * @title GrantStream
 * @dev Main contract for managing grant streams with milestone completion proof hashing
 * @notice This contract allows grant creators to set up streams with milestones that require proof completion
 */
contract GrantStream is Ownable, ReentrancyGuard {
    using Counters for Counters.Counter;
    using HashUtils for bytes32;

    Counters.Counter private _grantIds;
    Counters.Counter private _milestoneIds;

    enum MilestoneStatus { Pending, InProgress, Completed, Approved, Rejected }
    enum GrantStatus { Active, Paused, Completed, Cancelled }

    struct MilestoneRecord {
        uint256 id;
        uint256 grantId;
        string title;
        string description;
        uint256 amount;
        uint256 deadline;
        MilestoneStatus status;
        address grantee;
        uint256 completionTime;
        bytes32 proofHash; // SHA-256 hash of deliverable
        string proofMetadata; // Optional metadata about the proof (e.g., file type, URL)
        bool exists;
    }

    struct Grant {
        uint256 id;
        address creator;
        address grantee;
        uint256 totalAmount;
        uint256 releasedAmount;
        uint256 createdAt;
        uint256 lastUpdated;
        GrantStatus status;
        string title;
        string description;
        uint256[] milestoneIds;
        bool exists;
    }

    mapping(uint256 => Grant) public grants;
    mapping(uint256 => MilestoneRecord) public milestones;
    mapping(address => uint256[]) public granteeToGrants;
    mapping(address => uint256[]) public creatorToGrants;

    event GrantCreated(
        uint256 indexed grantId,
        address indexed creator,
        address indexed grantee,
        uint256 totalAmount,
        string title
    );

    event MilestoneCreated(
        uint256 indexed milestoneId,
        uint256 indexed grantId,
        string title,
        uint256 amount,
        uint256 deadline
    );

    event MilestoneProofSubmitted(
        uint256 indexed milestoneId,
        address indexed grantee,
        bytes32 proofHash,
        string proofMetadata
    );

    event MilestoneCompleted(
        uint256 indexed milestoneId,
        uint256 indexed grantId,
        address indexed grantee,
        uint256 amount
    );

    modifier onlyGrantee(uint256 _grantId) {
        require(grants[_grantId].grantee == msg.sender, "GrantStream: Only grantee can perform this action");
        _;
    }

    modifier onlyCreator(uint256 _grantId) {
        require(grants[_grantId].creator == msg.sender, "GrantStream: Only creator can perform this action");
        _;
    }

    modifier milestoneExists(uint256 _milestoneId) {
        require(milestones[_milestoneId].exists, "GrantStream: Milestone does not exist");
        _;
    }

    modifier grantExists(uint256 _grantId) {
        require(grants[_grantId].exists, "GrantStream: Grant does not exist");
        _;
    }

    constructor() {
        // Initialize counters
        _grantIds.increment();
        _milestoneIds.increment();
    }

    /**
     * @dev Creates a new grant stream
     * @param _grantee Address of the grant recipient
     * @param _totalAmount Total amount for the grant
     * @param _title Title of the grant
     * @param _description Description of the grant
     */
    function createGrant(
        address _grantee,
        uint256 _totalAmount,
        string memory _title,
        string memory _description
    ) external returns (uint256) {
        require(_grantee != address(0), "GrantStream: Grantee cannot be zero address");
        require(_totalAmount > 0, "GrantStream: Amount must be greater than 0");

        uint256 grantId = _grantIds.current();
        _grantIds.increment();

        grants[grantId] = Grant({
            id: grantId,
            creator: msg.sender,
            grantee: _grantee,
            totalAmount: _totalAmount,
            releasedAmount: 0,
            createdAt: block.timestamp,
            lastUpdated: block.timestamp,
            status: GrantStatus.Active,
            title: _title,
            description: _description,
            milestoneIds: new uint256[](0),
            exists: true
        });

        granteeToGrants[_grantee].push(grantId);
        creatorToGrants[msg.sender].push(grantId);

        emit GrantCreated(grantId, msg.sender, _grantee, _totalAmount, _title);
        return grantId;
    }

    /**
     * @dev Creates a milestone for a grant
     * @param _grantId ID of the grant
     * @param _title Title of the milestone
     * @param _description Description of the milestone
     * @param _amount Amount to be released upon completion
     * @param _deadline Deadline for milestone completion
     */
    function createMilestone(
        uint256 _grantId,
        string memory _title,
        string memory _description,
        uint256 _amount,
        uint256 _deadline
    ) external onlyCreator(_grantId) grantExists(_grantId) returns (uint256) {
        require(_amount > 0, "GrantStream: Amount must be greater than 0");
        require(_deadline > block.timestamp, "GrantStream: Deadline must be in the future");

        uint256 milestoneId = _milestoneIds.current();
        _milestoneIds.increment();

        milestones[milestoneId] = MilestoneRecord({
            id: milestoneId,
            grantId: _grantId,
            title: _title,
            description: _description,
            amount: _amount,
            deadline: _deadline,
            status: MilestoneStatus.Pending,
            grantee: grants[_grantId].grantee,
            completionTime: 0,
            proofHash: bytes32(0),
            proofMetadata: "",
            exists: true
        });

        grants[_grantId].milestoneIds.push(milestoneId);
        grants[_grantId].lastUpdated = block.timestamp;

        emit MilestoneCreated(milestoneId, _grantId, _title, _amount, _deadline);
        return milestoneId;
    }

    /**
     * @dev Submits proof for milestone completion with SHA-256 hash
     * @param _milestoneId ID of the milestone
     * @param _proofHash SHA-256 hash of the deliverable
     * @param _proofMetadata Optional metadata about the proof (file type, URL, etc.)
     */
    function submitProof(
        uint256 _milestoneId,
        bytes32 _proofHash,
        string memory _proofMetadata
    ) external milestoneExists(_milestoneId) nonReentrant {
        MilestoneRecord storage milestone = milestones[_milestoneId];
        
        require(milestone.grantee == msg.sender, "GrantStream: Only grantee can submit proof");
        require(milestone.status == MilestoneStatus.Pending || milestone.status == MilestoneStatus.InProgress, 
                "GrantStream: Milestone must be pending or in progress");
        require(_proofHash != bytes32(0), "GrantStream: Proof hash cannot be zero");
        require(block.timestamp <= milestone.deadline, "GrantStream: Milestone deadline has passed");

        // Update milestone with proof information
        milestone.proofHash = _proofHash;
        milestone.proofMetadata = _proofMetadata;
        milestone.status = MilestoneStatus.Completed;
        milestone.completionTime = block.timestamp;

        emit MilestoneProofSubmitted(_milestoneId, msg.sender, _proofHash, _proofMetadata);
        emit MilestoneCompleted(_milestoneId, milestone.grantId, msg.sender, milestone.amount);
    }

    /**
     * @dev Verifies a proof hash against provided data
     * @param _proofHash The stored proof hash
     * @param _data The original data to verify
     * @return bool True if the hash matches
     */
    function verifyProof(bytes32 _proofHash, bytes memory _data) external pure returns (bool) {
        return HashUtils.sha256(_data) == _proofHash;
    }

    /**
     * @dev Gets milestone details including proof information
     * @param _milestoneId ID of the milestone
     * @return MilestoneRecord with all details
     */
    function getMilestone(uint256 _milestoneId) external view milestoneExists(_milestoneId) returns (MilestoneRecord memory) {
        return milestones[_milestoneId];
    }

    /**
     * @dev Gets grant details
     * @param _grantId ID of the grant
     * @return Grant with all details
     */
    function getGrant(uint256 _grantId) external view grantExists(_grantId) returns (Grant memory) {
        return grants[_grantId];
    }

    /**
     * @dev Gets all milestones for a grant
     * @param _grantId ID of the grant
     * @return Array of milestone IDs
     */
    function getGrantMilestones(uint256 _grantId) external view grantExists(_grantId) returns (uint256[] memory) {
        return grants[_grantId].milestoneIds;
    }

    /**
     * @dev Gets the immutable audit trail for a milestone
     * @param _milestoneId ID of the milestone
     * @return completionTime, proofHash, proofMetadata
     */
    function getMilestoneAuditTrail(uint256 _milestoneId) external view milestoneExists(_milestoneId) 
        returns (uint256 completionTime, bytes32 proofHash, string memory proofMetadata) {
        MilestoneRecord memory milestone = milestones[_milestoneId];
        return (milestone.completionTime, milestone.proofHash, milestone.proofMetadata);
    }
}
