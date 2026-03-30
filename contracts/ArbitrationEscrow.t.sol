// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "forge-std/Test.sol";
import "../contracts/ArbitrationEscrow.sol";
import "../contracts/GrantStreamWithArbitration.sol";
import "../contracts/SustainabilityFund.sol";
import "../contracts/Web3Courtroom.sol";

contract ArbitrationEscrowTest is Test {
    ArbitrationEscrow public arbitrationEscrow;
    GrantStreamWithArbitration public grantStream;
    SustainabilityFund public sustainabilityFund;
    Web3Courtroom public courtroom;
    
    address public owner = address(0x1);
    address public funder = address(0x2);
    address public grantee = address(0x3);
    address public arbitrator = address(0x4);
    address public treasury = address(0x5);
    
    uint256 public grantId;
    uint256 public disputeId;
    uint256 public arbitratorId;
    
    event DisputeRaised(
        uint256 indexed disputeId,
        uint256 indexed grantId,
        address indexed funder,
        address grantee,
        uint256 disputedAmount,
        string evidence,
        string reason
    );
    
    function setUp() public {
        vm.startPrank(owner);
        
        // Deploy contracts
        sustainabilityFund = new SustainabilityFund(treasury);
        arbitrationEscrow = new ArbitrationEscrow();
        grantStream = new GrantStreamWithArbitration(address(sustainabilityFund), address(arbitrationEscrow));
        courtroom = new Web3Courtroom(address(arbitrationEscrow));
        
        // Set GrantStream contract in ArbitrationEscrow
        arbitrationEscrow.setGrantStreamContract(address(grantStream));
        
        // Register and approve arbitrator
        arbitratorId = arbitrationEscrow.registerArbitrator(
            arbitrator,
            "Test Legal Firm",
            "Delaware, USA"
        );
        arbitrationEscrow.setArbitratorApproval(arbitratorId, true);
        
        // Register arbitrator profile in courtroom
        courtroom.registerArbitratorProfile(
            arbitrator,
            "Test Legal Firm",
            "Delaware, USA",
            "Software Development",
            "https://testlegal.com",
            "contact@testlegal.com"
        );
        
        vm.stopPrank();
        
        // Create a grant for testing
        vm.startPrank(funder);
        grantId = grantStream.createGrant{value: 10 ether}(grantee);
        vm.stopPrank();
    }
    
    function testCreateGrant() public {
        assertEq(grantStream.grants(grantId).funder, funder);
        assertEq(grantStream.grants(grantId).recipient, grantee);
        assertEq(grantStream.grants(grantId).balance, 10 ether);
        assertEq(uint8(grantStream.grants(grantId).status), uint8(GrantStreamWithArbitration.GrantStatus.Active));
    }
    
    function testRaiseDispute() public {
        vm.startPrank(funder);
        
        // Raise dispute for 5 ether
        string memory evidence = "QmTestEvidenceHash";
        string memory reason = "Project not delivered as promised";
        
        vm.expectEmit(true, true, true, true);
        emit DisputeRaised(disputeId, grantId, funder, grantee, 5 ether, evidence, reason);
        
        grantStream.raiseDispute{value: 5 ether}(grantId, 5 ether, evidence, reason);
        
        vm.stopPrank();
        
        // Check dispute was created
        disputeId = grantStream.getActiveDisputeId(grantId);
        assertGt(disputeId, 0);
        
        ArbitrationEscrow.Dispute memory dispute = arbitrationEscrow.getDispute(disputeId);
        assertEq(dispute.grantId, grantId);
        assertEq(dispute.funder, funder);
        assertEq(dispute.grantee, grantee);
        assertEq(dispute.disputedAmount, 5 ether);
        assertEq(uint8(dispute.status), uint8(ArbitrationEscrow.DisputeStatus.Pending));
        
        // Check grant status updated
        assertEq(uint8(grantStream.grants(grantId).status), uint8(GrantStreamWithArbitration.GrantStatus.InDispute));
        assertEq(grantStream.grants(grantId).disputedAmount, 5 ether);
        assertEq(grantStream.grants(grantId).balance, 5 ether); // 10 - 5 disputed
    }
    
    function testAcceptDispute() public {
        _raiseDispute();
        
        vm.startPrank(arbitrator);
        
        arbitrationEscrow.acceptDispute(disputeId, arbitratorId);
        
        vm.stopPrank();
        
        // Check dispute status
        ArbitrationEscrow.Dispute memory dispute = arbitrationEscrow.getDispute(disputeId);
        assertEq(uint8(dispute.status), uint8(ArbitrationEscrow.DisputeStatus.InArbitration));
        assertEq(dispute.arbitrator, arbitrator);
        assertEq(dispute.arbitrationFee, (5 ether * 200) / 10000); // 2% fee
        
        // Check arbitrator stats
        ArbitrationEscrow.Arbitrator memory arb = arbitrationEscrow.getArbitrator(arbitratorId);
        assertEq(arb.activeCases, 1);
        assertEq(arb.totalCases, 1);
    }
    
    function testIssueDecision() public {
        _raiseDispute();
        _acceptDispute();
        
        vm.startPrank(arbitrator);
        
        string memory ruling = "After reviewing evidence, partial refund granted to funder";
        
        arbitrationEscrow.issueDecision(
            disputeId,
            ArbitrationEscrow.ArbitrationDecision.FavorFunder,
            3 ether, // funder award
            1.95 ether, // grantee award (5 ether - 3 ether - 0.05 ether fee)
            ruling
        );
        
        vm.stopPrank();
        
        // Check dispute resolved
        ArbitrationEscrow.Dispute memory dispute = arbitrationEscrow.getDispute(disputeId);
        assertEq(uint8(dispute.status), uint8(ArbitrationEscrow.DisputeStatus.Resolved));
        assertEq(uint8(dispute.decision), uint8(ArbitrationEscrow.ArbitrationDecision.FavorFunder));
        assertEq(dispute.funderAward, 3 ether);
        assertEq(dispute.granteeAward, 1.95 ether);
        assertEq(dispute.arbitrationFee, 0.05 ether);
        
        // Check balances
        assertEq(funder.balance, 3 ether);
        assertEq(grantee.balance, 1.95 ether);
        assertEq(arbitrator.balance, 0.05 ether);
    }
    
    function testWeb3CourtroomIntegration() public {
        _raiseDispute();
        
        vm.startPrank(funder);
        
        // Create court case
        string memory title = "Software Development Dispute";
        string memory description = "Dispute over incomplete software delivery";
        
        courtroom.createCourtCase(disputeId, title, description, true);
        
        // Submit additional evidence
        courtroom.submitEvidence(disputeId, "QmAdditionalEvidence");
        
        vm.stopPrank();
        
        // Check public case information
        (
            uint256 caseDisputeId,
            uint256 caseGrantId,
            string memory caseTitle,
            string memory caseDescription,
            uint256 filingDate,
            uint256 lastUpdate,
            ArbitrationEscrow.DisputeStatus status,
            uint256 stakeAmount,
            bool isPublic
        ) = courtroom.getPublicCase(disputeId);
        
        assertEq(caseDisputeId, disputeId);
        assertEq(caseGrantId, grantId);
        assertEq(caseTitle, title);
        assertEq(caseDescription, description);
        assertEq(stakeAmount, 5 ether);
        assertTrue(isPublic);
    }
    
    function testArbitratorReputation() public {
        _raiseDispute();
        _acceptDispute();
        _issueDecision();
        
        // Check reputation increased
        ArbitrationEscrow.Arbitrator memory arb = arbitrationEscrow.getArbitrator(arbitratorId);
        assertEq(arb.reputationScore, 510); // Started at 500, +10 for successful decision
        assertEq(arb.activeCases, 0); // Should be 0 after resolution
    }
    
    function testUnauthorizedDisputeRaise() public {
        vm.startPrank(grantee);
        
        vm.expectRevert("GrantStream: not funder");
        grantStream.raiseDispute{value: 1 ether}(grantId, 1 ether, "evidence", "reason");
        
        vm.stopPrank();
    }
    
    function testInvalidDisputeAmount() public {
        vm.startPrank(funder);
        
        // Try to dispute more than available
        vm.expectRevert("GrantStream: invalid dispute amount");
        grantStream.raiseDispute{value: 15 ether}(grantId, 15 ether, "evidence", "reason");
        
        vm.stopPrank();
    }
    
    function testDuplicateDispute() public {
        _raiseDispute();
        
        vm.startPrank(funder);
        
        vm.expectRevert("GrantStream: dispute already active");
        grantStream.raiseDispute{value: 1 ether}(grantId, 1 ether, "evidence", "reason");
        
        vm.stopPrank();
    }
    
    function testUnauthorizedArbitrator() public {
        _raiseDispute();
        
        address unauthorizedArbitrator = address(0x6);
        vm.startPrank(unauthorizedArbitrator);
        
        vm.expectRevert("ArbitrationEscrow: Not arbitrator");
        arbitrationEscrow.acceptDispute(disputeId, arbitratorId);
        
        vm.stopPrank();
    }
    
    function testInvalidDecisionAmounts() public {
        _raiseDispute();
        _acceptDispute();
        
        vm.startPrank(arbitrator);
        
        // Try to award more than disputed amount
        vm.expectRevert("ArbitrationEscrow: Awards exceed disputed amount");
        arbitrationEscrow.issueDecision(
            disputeId,
            ArbitrationEscrow.ArbitrationDecision.FavorFunder,
            4 ether,
            2 ether, // Total 6 ether > 5 ether disputed
            "ruling"
        );
        
        vm.stopPrank();
    }
    
    function testEmergencyPause() public {
        vm.startPrank(owner);
        
        arbitrationEscrow.pause();
        
        vm.expectRevert("Enforced pause");
        arbitrationEscrow.raiseDispute{value: 1 ether}(1, funder, grantee, 1 ether, "evidence", "reason");
        
        vm.stopPrank();
    }
    
    function testGetApprovedArbitrators() public {
        uint256[] memory approvedArbitrators = arbitrationEscrow.getApprovedArbitrators();
        assertEq(approvedArbitrators.length, 1);
        assertEq(approvedArbitrators[0], arbitratorId);
    }
    
    function testCourtroomStatistics() public {
        _raiseDispute();
        courtroom.createCourtCase(disputeId, "Test Case", "Test Description", true);
        
        (
            uint256 totalCases,
            uint256 totalValueDisputed,
            uint256 averageResolutionTime,
            uint256 publicCasesCount,
            uint256 activeCasesCount,
            uint256 resolvedCasesCount,
            uint256 pendingCasesCount
        ) = courtroom.getStatistics();
        
        assertEq(totalCases, 1);
        assertEq(totalValueDisputed, 5 ether);
        assertEq(publicCasesCount, 1);
        assertEq(pendingCasesCount, 1);
        assertEq(activeCasesCount, 0);
        assertEq(resolvedCasesCount, 0);
    }
    
    // ─── Helper Functions ──────────────────────────────────────────────────────
    
    function _raiseDispute() internal {
        vm.startPrank(funder);
        grantStream.raiseDispute{value: 5 ether}(grantId, 5 ether, "QmTestEvidenceHash", "Project not delivered");
        disputeId = grantStream.getActiveDisputeId(grantId);
        vm.stopPrank();
    }
    
    function _acceptDispute() internal {
        vm.startPrank(arbitrator);
        arbitrationEscrow.acceptDispute(disputeId, arbitratorId);
        vm.stopPrank();
    }
    
    function _issueDecision() internal {
        vm.startPrank(arbitrator);
        arbitrationEscrow.issueDecision(
            disputeId,
            ArbitrationEscrow.ArbitrationDecision.FavorFunder,
            3 ether,
            1.95 ether,
            "Partial refund granted"
        );
        vm.stopPrank();
    }
}
