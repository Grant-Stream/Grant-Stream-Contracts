// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "forge-std/Script.sol";
import "../src/GrantStream.sol";

contract DeployGrantStream is Script {
    GrantStream public grantStream;

    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        address deployer = vm.addr(deployerPrivateKey);

        vm.startBroadcast(deployerPrivateKey);

        grantStream = new GrantStream();

        vm.stopBroadcast();

        console.log("GrantStream deployed at:", address(grantStream));
        console.log("Deployed by:", deployer);
        console.log("Transaction hash:", vm.getTransactionHash());
    }
}
