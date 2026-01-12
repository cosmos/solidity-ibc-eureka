// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/*
    This script is used for deploying the demo contracts on testnets
*/

// solhint-disable custom-errors,gas-custom-errors

import { Script } from "forge-std/Script.sol";

import { AttestationLightClient } from "../contracts/light-clients/attestation/AttestationLightClient.sol";

contract AttestationLCDeploy is Script {
    function run() public {
        address[] memory attestors = new address[](1);
        attestors[0] = 0x5Bc9C9baB1B5Df2d114164437261839D207AF061;

        vm.startBroadcast();

        // Deploy Attestor Light Client with proxy
        new AttestationLightClient(attestors, 1, 7371, 1_756_814_141, address(0));

        vm.stopBroadcast();
    }
}
