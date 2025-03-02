// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Deployments } from "../helpers/Deployments.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { TimelockController } from "@openzeppelin-contracts/governance/TimelockController.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";

// solhint-disable custom-errors,gas-custom-errors

library UpgradeProxiedICS26Router {
    using stdJson for string;

    // If you are using a TimelockController as the administrator address on the router,
    // you will have to call this function twice - once to schedule the upgrade and once to execute it.
    function upgrade(Deployments.ProxiedICS26RouterUpgrade memory routerUpgrade) public {
        if (routerUpgrade.timeLockAdmin == payable(address(0))) {
            return _upgradeDirect(routerUpgrade);
        }

        return _upgradeTimelock(routerUpgrade);
    }

    function _upgradeTimelock(Deployments.ProxiedICS26RouterUpgrade memory routerUpgrade) internal {
        TimelockController tlc = TimelockController(routerUpgrade.timeLockAdmin);
        uint256 minDelay = tlc.getMinDelay();
        bytes32 hash = tlc.hashOperation(
            address(routerUpgrade.proxy),
            0,
            abi.encodeWithSignature("upgradeToAndCall(address,bytes)", routerUpgrade.newImplementation, bytes("")),
            0,
            0
        );

        if (tlc.isOperationDone(hash)) {
            revert("Upgrade has been executed");
        } else if (tlc.isOperationPending(hash) && !tlc.isOperationReady(hash)) {
            revert("The upgrade operation is pending");
        } else if (tlc.isOperationReady(hash)) {
            tlc.execute(
                address(routerUpgrade.proxy),
                0,
                abi.encodeWithSignature("upgradeToAndCall(address,bytes)", routerUpgrade.newImplementation, bytes("")),
                0,
                0
            );
            return;
        }

        tlc.schedule(
            address(routerUpgrade.proxy),
            0,
            abi.encodeWithSignature("upgradeToAndCall(address,bytes)", routerUpgrade.newImplementation, bytes("")),
            0,
            0,
            minDelay
        );
    }

    function _upgradeDirect(Deployments.ProxiedICS26RouterUpgrade memory routerUpgrade) internal {
        ICS26Router router = ICS26Router(routerUpgrade.proxy);
        router.upgradeToAndCall(routerUpgrade.newImplementation, bytes(""));
    }
}
