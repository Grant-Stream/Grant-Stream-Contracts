// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "forge-std/Script.sol";
import "../contracts/GrantStream.sol";
import "../contracts/GrantStreamProxy.sol";
import "../contracts/DAOUpgradeGovernance.sol";
import "../contracts/SustainabilityFund.sol";

/**
 * @notice Deploys the full Wasm-Rotation stack:
 *   1. SustainabilityFund  (if not already deployed)
 *   2. GrantStream          (initial logic implementation)
 *   3. DAOUpgradeGovernance (placeholder — needs proxy address first)
 *   4. GrantStreamProxy     (proxy pointing at GrantStream, governed by DAO)
 *
 * Set env vars before running:
 *   PRIVATE_KEY, TREASURY_ADDRESS, DAO_MEMBER_1, DAO_MEMBER_2
 *
 * forge script script/DeployProxy.s.sol --rpc-url <RPC_URL> --broadcast
 */
contract DeployProxy is Script {
    function run() external {
        uint256 pk       = vm.envUint("PRIVATE_KEY");
        address treasury = vm.envAddress("TREASURY_ADDRESS");

        vm.startBroadcast(pk);

        // 1. Sustainability fund
        SustainabilityFund fund = new SustainabilityFund(treasury);

        // 2. Initial logic implementation
        GrantStream logic = new GrantStream(address(fund));

        // 3. DAO — bootstrapped with deployer; proxy address set after step 4
        //    We deploy a temporary DAO pointing at address(0) then upgrade,
        //    OR deploy proxy first with deployer as DAO, then hand off.
        //    Here we use the two-step approach: deployer acts as DAO initially.
        address deployer = vm.addr(pk);

        // 4. Proxy — deployer is DAO for now
        GrantStreamProxy proxy = new GrantStreamProxy(address(logic), deployer);

        // 5. Real DAO now that we have the proxy address
        DAOUpgradeGovernance dao = new DAOUpgradeGovernance(address(proxy));

        // Optionally add extra members from env (skip if not set)
        // dao.addMember(vm.envAddress("DAO_MEMBER_1"));

        vm.stopBroadcast();

        console.log("SustainabilityFund :", address(fund));
        console.log("GrantStream (logic) :", address(logic));
        console.log("GrantStreamProxy    :", address(proxy));
        console.log("DAOUpgradeGovernance:", address(dao));
        console.log("");
        console.log("NEXT STEP: call proxy.upgradeLogic() or transfer DAO");
        console.log("ownership so the DAO contract is the sole upgrade authority.");
    }
}
