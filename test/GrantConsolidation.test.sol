// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "forge-std/Test.sol";
import "../contracts/GrantStream.sol";
import "../contracts/GrantConsolidator.sol";
import "../contracts/SustainabilityFund.sol";

contract GrantConsolidationTest is Test {
    GrantStream public grantStream;
    SustainabilityFund public fund;
    GrantConsolidator public consolidator;

    address funder1 = address(0x111);
    address funder2 = address(0x222);
    address grantee = address(0x333);
    address teamOwner = address(0x444);

    function setUp() public {
        vm.deal(funder1, 100 ether);
        vm.deal(funder2, 100 ether);
        
        fund = new SustainabilityFund(address(0xdead));
        grantStream = new GrantStream(address(fund));
        consolidator = new GrantConsolidator(address(grantStream), teamOwner);
    }

    function testConsolidationFlow() public {
        // 1. Create two grants for the same grantee
        vm.startPrank(funder1);
        uint256 grantId1 = grantStream.createGrant{value: 10 ether}(grantee);
        vm.stopPrank();

        vm.startPrank(funder2);
        uint256 grantId2 = grantStream.createGrant{value: 20 ether}(grantee);
        vm.stopPrank();

        // 2. Grantee "merges" them into the vault/consolidator
        vm.startPrank(grantee);
        grantStream.updateRecipient(grantId1, address(consolidator));
        grantStream.updateRecipient(grantId2, address(consolidator));
        vm.stopPrank();

        // 3. Team owner registers the grants in the consolidator
        vm.startPrank(teamOwner);
        consolidator.addGrant(grantId1);
        consolidator.addGrant(grantId2);

        // 4. Batch claim from both grants
        uint256[] memory ids = new uint256[](2);
        ids[0] = grantId1;
        ids[1] = grantId2;
        uint256[] memory amounts = new uint256[](2);
        amounts[0] = 5 ether;
        amounts[1] = 10 ether;

        consolidator.batchClaim(ids, amounts);

        // 5. Verify accounting
        (uint256 totalVaultBalance, uint256 numGrants, uint256 history) = consolidator.getVaultSummary();
        assertEq(totalVaultBalance, 15 ether);
        assertEq(numGrants, 2);
        assertEq(history, 15 ether);

        assertEq(consolidator.totalReceivedPerGrantor(funder1), 5 ether);
        assertEq(consolidator.totalReceivedPerGrantor(funder2), 10 ether);

        // 6. Withdraw to team owner
        uint256 beforeBalance = teamOwner.balance;
        consolidator.withdraw(payable(teamOwner), 15 ether);
        assertEq(teamOwner.balance, beforeBalance + 15 ether);
        assertEq(address(consolidator).balance, 0);

        vm.stopPrank();
    }
}
