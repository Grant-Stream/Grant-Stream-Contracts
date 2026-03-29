// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "forge-std/Test.sol";
import "../src/GrantStream.sol";
import "../src/libraries/HashUtils.sol";

contract GrantStreamTest is Test {
    GrantStream public grantStream;
    
    address public creator = address(0x1);
    address public grantee = address(0x2);
    address public other = address(0x3);
    
    uint256 public constant TOTAL_AMOUNT = 1000 ether;
    uint256 public constant MILESTONE_AMOUNT = 250 ether;
    uint256 public constant DEADLINE = block.timestamp + 30 days;
    
    uint256 public grantId;
    uint256 public milestoneId;
    
    event MilestoneProofSubmitted(
        uint256 indexed milestoneId,
        address indexed grantee,
        bytes32 proofHash,
        string proofMetadata
    );
    
    function setUp() public {
        grantStream = new GrantStream();
        
        vm.startPrank(creator);
        grantId = grantStream.createGrant(
            grantee,
            TOTAL_AMOUNT,
            "Test Grant",
            "A test grant for milestone completion"
        );
        
        milestoneId = grantStream.createMilestone(
            grantId,
            "Test Milestone",
            "Complete the test deliverable",
            MILESTONE_AMOUNT,
            DEADLINE
        );
        vm.stopPrank();
    }
    
    function testSubmitProof_ValidHash() public {
        bytes memory deliverable = "This is a test deliverable PDF content";
        bytes32 proofHash = sha256(deliverable);
        string memory metadata = "PDF Report - Q1 2024";
        
        vm.startPrank(grantee);
        
        vm.expectEmit(true, true, false, true);
        emit MilestoneProofSubmitted(milestoneId, grantee, proofHash, metadata);
        
        grantStream.submitProof(milestoneId, proofHash, metadata);
        
        vm.stopPrank();
        
        // Verify milestone is completed
        GrantStream.MilestoneRecord memory milestone = grantStream.getMilestone(milestoneId);
        assertEq(milestone.status, GrantStream.MilestoneStatus.Completed);
        assertEq(milestone.proofHash, proofHash);
        assertEq(milestone.proofMetadata, metadata);
        assertTrue(milestone.completionTime > 0);
    }
    
    function testSubmitProof_GitHubRelease() public {
        string memory repoUrl = "https://github.com/user/repo";
        string memory releaseTag = "v1.0.0";
        string memory commitHash = "abc123def456";
        string memory releaseNotes = "Initial release with all features";
        
        bytes32 proofHash = HashUtils.createReleaseHash(
            repoUrl,
            releaseTag,
            commitHash,
            releaseNotes
        );
        
        string memory metadata = "GitHub Release - v1.0.0";
        
        vm.startPrank(grantee);
        grantStream.submitProof(milestoneId, proofHash, metadata);
        vm.stopPrank();
        
        GrantStream.MilestoneRecord memory milestone = grantStream.getMilestone(milestoneId);
        assertEq(milestone.proofHash, proofHash);
        assertEq(milestone.proofMetadata, metadata);
    }
    
    function testSubmitProof_FileHash() public {
        string memory fileName = "Q1_Report.pdf";
        bytes memory fileContent = "PDF content here";
        string memory fileType = "PDF";
        
        bytes32 proofHash = HashUtils.createFileHash(fileName, fileContent, fileType);
        
        vm.startPrank(grantee);
        grantStream.submitProof(milestoneId, proofHash, "Quarterly Report PDF");
        vm.stopPrank();
        
        GrantStream.MilestoneRecord memory milestone = grantStream.getMilestone(milestoneId);
        assertEq(milestone.proofHash, proofHash);
    }
    
    function testSubmitProof_RevertsIfNotGrantee() public {
        bytes32 proofHash = sha256("test content");
        
        vm.startPrank(other);
        vm.expectRevert("GrantStream: Only grantee can submit proof");
        grantStream.submitProof(milestoneId, proofHash, "test metadata");
        vm.stopPrank();
    }
    
    function testSubmitProof_RevertsIfZeroHash() public {
        vm.startPrank(grantee);
        vm.expectRevert("GrantStream: Proof hash cannot be zero");
        grantStream.submitProof(milestoneId, bytes32(0), "test metadata");
        vm.stopPrank();
    }
    
    function testSubmitProof_RevertsIfDeadlinePassed() public {
        // Warp to after deadline
        vm.warp(DEADLINE + 1);
        
        bytes32 proofHash = sha256("test content");
        
        vm.startPrank(grantee);
        vm.expectRevert("GrantStream: Milestone deadline has passed");
        grantStream.submitProof(milestoneId, proofHash, "test metadata");
        vm.stopPrank();
    }
    
    function testSubmitProof_RevertsIfAlreadyCompleted() public {
        bytes32 proofHash = sha256("test content");
        
        // Complete milestone first
        vm.startPrank(grantee);
        grantStream.submitProof(milestoneId, proofHash, "test metadata");
        
        // Try to submit again
        vm.expectRevert("GrantStream: Milestone must be pending or in progress");
        grantStream.submitProof(milestoneId, sha256("different content"), "different metadata");
        vm.stopPrank();
    }
    
    function testVerifyProof() public {
        bytes memory deliverable = "Test deliverable content";
        bytes32 proofHash = sha256(deliverable);
        
        // Submit proof
        vm.startPrank(grantee);
        grantStream.submitProof(milestoneId, proofHash, "Test deliverable");
        vm.stopPrank();
        
        // Verify proof
        bool isValid = grantStream.verifyProof(proofHash, deliverable);
        assertTrue(isValid);
        
        // Verify with wrong content
        bool isInvalid = grantStream.verifyProof(proofHash, "Wrong content");
        assertFalse(isInvalid);
    }
    
    function testGetMilestoneAuditTrail() public {
        bytes32 proofHash = sha256("audit trail test");
        string memory metadata = "Audit Trail Test";
        
        vm.startPrank(grantee);
        grantStream.submitProof(milestoneId, proofHash, metadata);
        vm.stopPrank();
        
        (uint256 completionTime, bytes32 storedProofHash, string memory storedMetadata) = 
            grantStream.getMilestoneAuditTrail(milestoneId);
        
        assertTrue(completionTime > 0);
        assertEq(storedProofHash, proofHash);
        assertEq(storedMetadata, metadata);
    }
    
    function testMultipleMilestonesWithProofs() public {
        // Create additional milestones
        vm.startPrank(creator);
        uint256 milestone2Id = grantStream.createMilestone(
            grantId,
            "Second Milestone",
            "Complete second deliverable",
            MILESTONE_AMOUNT,
            DEADLINE
        );
        
        uint256 milestone3Id = grantStream.createMilestone(
            grantId,
            "Third Milestone",
            "Complete third deliverable",
            MILESTONE_AMOUNT,
            DEADLINE
        );
        vm.stopPrank();
        
        // Submit proofs for all milestones
        vm.startPrank(grantee);
        
        bytes32 proof1 = sha256("First deliverable");
        bytes32 proof2 = sha256("Second deliverable");
        bytes32 proof3 = sha256("Third deliverable");
        
        grantStream.submitProof(milestoneId, proof1, "First proof");
        grantStream.submitProof(milestone2Id, proof2, "Second proof");
        grantStream.submitProof(milestone3Id, proof3, "Third proof");
        
        vm.stopPrank();
        
        // Verify all milestones are completed
        GrantStream.MilestoneRecord memory milestone1 = grantStream.getMilestone(milestoneId);
        GrantStream.MilestoneRecord memory milestone2 = grantStream.getMilestone(milestone2Id);
        GrantStream.MilestoneRecord memory milestone3 = grantStream.getMilestone(milestone3Id);
        
        assertEq(milestone1.status, GrantStream.MilestoneStatus.Completed);
        assertEq(milestone2.status, GrantStream.MilestoneStatus.Completed);
        assertEq(milestone3.status, GrantStream.MilestoneStatus.Completed);
        
        assertEq(milestone1.proofHash, proof1);
        assertEq(milestone2.proofHash, proof2);
        assertEq(milestone3.proofHash, proof3);
    }
    
    function testHashUtilsIntegration() public {
        // Test various hash utility functions
        string memory testData = "Test data for hashing";
        bytes32 directHash = sha256(bytes(testData));
        bytes32 utilHash = HashUtils.sha256(testData);
        
        assertEq(directHash, utilHash);
        
        // Test concatenated hashing
        string memory part1 = "Part 1";
        string memory part2 = "Part 2";
        bytes32 concatHash = HashUtils.sha256Concat(bytes(part1), bytes(part2));
        bytes32 expectedHash = sha256(abi.encodePacked(part1, part2));
        
        assertEq(concatHash, expectedHash);
        
        // Test file hash creation
        bytes32 fileHash = HashUtils.createFileHash("test.pdf", "content", "PDF");
        bytes32 expectedFileHash = sha256(abi.encodePacked("test.pdf", "content", "PDF"));
        
        assertEq(fileHash, expectedFileHash);
    }
}
