// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

import { stdJson } from "forge-std/StdJson.sol";

library DeployLib {
    using stdJson for string;

    struct DeploymentConfigJson {
        address timelockAdminAddress;
        address portCustomizerAddress;
        address ics20PauserAddress;
        address permit2Address;
    }

    function loadDeploymentConfigFromJson(string memory deploymentConfigJson) internal pure returns (DeploymentConfigJson memory) {
        address timelockAdminAddress = deploymentConfigJson.readAddress(".timelockAdminAddress");
        address portCustomizerAddress = deploymentConfigJson.readAddress(".portCustomizerAddress");
        address ics20PauserAddress = deploymentConfigJson.readAddress(".ics20PauserAddress");
        address permit2Address = deploymentConfigJson.readAddress(".permit2Address");

        return DeploymentConfigJson(
            timelockAdminAddress,
            portCustomizerAddress,
            ics20PauserAddress,
            permit2Address
        );
    }
}
