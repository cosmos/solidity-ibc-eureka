// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/*
    This script is used for deploying the demo contracts on testnets
*/

// solhint-disable custom-errors,gas-custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";

import { ICS26Router } from "../contracts/ICS26Router.sol";
import { ICS27GMP } from "../contracts/ICS27GMP.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ICS27Lib } from "../contracts/utils/ICS27Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { ICS27Account } from "../contracts/utils/ICS27Account.sol";
import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";
import { IBCXERC20 } from "../contracts/demo/IBCXERC20.sol";
import { AttestorLightClient } from "../contracts/light-clients/AttestorLightClient.sol";

contract AttestorLCDeploy is Script {
    using stdJson for string;

    string internal constant DEPLOYMENTS_DIR = "./scripts/deployments/";

    // solhint-disable-next-line function-max-lines
    function run() public {
	address[] memory attestors = new address[](1);
	attestors[0] = 0x5Bc9C9baB1B5Df2d114164437261839D207AF061;

        vm.startBroadcast();

	// Deploy Attestor Light Client with proxy
	new AttestorLightClient(
		attestors,
		1,
		7371,
		1756814141,
		address(0)
	);

        vm.stopBroadcast();
    }
}

