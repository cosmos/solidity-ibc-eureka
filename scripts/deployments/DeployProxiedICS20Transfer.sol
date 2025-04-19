// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

// solhint-disable-next-line no-global-import
import "forge-std/console.sol";

import { Script } from "forge-std/Script.sol";
import { Deployments } from "../helpers/Deployments.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBeacon } from "@openzeppelin-contracts/proxy/beacon/IBeacon.sol";
import { ERC1967Utils } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Utils.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../contracts/utils/Escrow.sol";
import { IBCPausableUpgradeable } from "../../contracts/utils/IBCPausableUpgradeable.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";


abstract contract DeployProxiedICS20Transfer is Deployments {
    function deployProxiedICS20Transfer(ProxiedICS20TransferDeployment memory deployment) public returns (ERC1967Proxy) {
        ERC1967Proxy transferProxy = new ERC1967Proxy(
            deployment.implementation,
            abi.encodeCall(
                ICS20Transfer.initialize,
                (
                    deployment.ics26Router,
                    deployment.escrowImplementation,
                    deployment.ibcERC20Implementation,
                    deployment.permit2
                )
            )
        );

        console.log("Deployed ICS20Transfer at address: ", address(transferProxy));

        ICS20Transfer ics20Transfer = ICS20Transfer(address(transferProxy));

        if (deployment.pausers.length != 0) {
            for (uint32 i = 0; i < deployment.pausers.length; i++) {
                address pauser = deployment.pausers[i];
                console.log("Granting pauser role to: ", pauser);
                ics20Transfer.grantPauserRole(pauser);
            }
        }

        if (deployment.unpausers.length != 0) {
            for (uint32 i = 0; i < deployment.unpausers.length; i++) {
                address unpauser = deployment.unpausers[i];
                console.log("Granting unpauser role to: ", unpauser);
                ics20Transfer.grantUnpauserRole(unpauser);
            }
        }

        if (deployment.tokenOperator != address(0)) {
            address tokenOperator = deployment.tokenOperator;
            console.log("Granting tokenOperator role to: ", tokenOperator);
            ics20Transfer.grantTokenOperatorRole(tokenOperator);
        }

        return transferProxy;
    }
}

contract DeployProxiedICS20TransferScript is DeployProxiedICS20Transfer, Script {
    function getImplementation(address proxy) internal view returns (address) {
        return address(uint160(uint256(vm.load(proxy, ERC1967Utils.IMPLEMENTATION_SLOT))));
    }

    function verify(ProxiedICS20TransferDeployment memory deployment) internal view {
        ERC1967Proxy transferProxy = ERC1967Proxy(payable(deployment.proxy));

        vm.assertEq(
            getImplementation(address(transferProxy)),
            deployment.implementation,
            "implementation addresses don't match"
        );

        ICS20Transfer ics20Transfer = ICS20Transfer(deployment.proxy);

        vm.assertEq(
            ics20Transfer.ics26(),
            deployment.ics26Router,
            "ics26Router addresses don't match"
        );

        vm.assertEq(
            IBeacon(ics20Transfer.getEscrowBeacon()).implementation(),
            deployment.escrowImplementation,
            "escrow addresses don't match"
        );

        vm.assertEq(
            IBeacon(ics20Transfer.getIBCERC20Beacon()).implementation(),
            deployment.ibcERC20Implementation,
            "ibcERC20 addresses don't match"
        );

        vm.assertEq(
            ics20Transfer.getPermit2(),
            deployment.permit2,
            "permit2 addresses don't match"
        );

        IICS26Router ics26Router = IICS26Router(deployment.ics26Router);
        address transferApp = address(ics26Router.getIBCApp(ICS20Lib.DEFAULT_PORT_ID));
        vm.assertEq(
            transferApp,
            deployment.proxy,
            "transfer app address doesn't match with the one in ics26Router"
        );

        for (uint256 i = 0; i < deployment.pausers.length; i++) {
            IBCPausableUpgradeable ipu = IBCPausableUpgradeable(address(transferProxy));

            vm.assertTrue(
                ipu.hasRole(ipu.PAUSER_ROLE(), deployment.pausers[i]),
                "pauser address doesn't have pauser role"
            );
        }

        for (uint256 i = 0; i < deployment.unpausers.length; i++) {
            IBCPausableUpgradeable ipu = IBCPausableUpgradeable(address(transferProxy));

            vm.assertTrue(
                ipu.hasRole(ipu.UNPAUSER_ROLE(), deployment.unpausers[i]),
                "unpauser address doesn't have unpauser role"
            );
        }
    }

    function run() public returns (address){
        string memory root = vm.projectRoot();
        string memory deployEnv = vm.envString("DEPLOYMENT_ENV");
        string memory path = string.concat(root, DEPLOYMENT_DIR, "/", deployEnv, "/", Strings.toString(block.chainid), ".json");
        string memory json = vm.readFile(path);

        bool verifyOnly = vm.envOr("VERIFY_ONLY", false);

        ProxiedICS20TransferDeployment memory deployment = loadProxiedICS20TransferDeployment(vm, json);

        if ((deployment.implementation != address(0) || deployment.proxy != address(0)) || verifyOnly) {
            verify(deployment);
            return deployment.proxy;
        }

        if (deployment.ics26Router == address(0)) {
            revert("ICS26Router not set");
        }

        vm.startBroadcast();

        if (deployment.implementation == address(0)) {
            deployment.implementation = address(new ICS20Transfer());
        }

        if (deployment.ibcERC20Implementation == address(0)) {
            deployment.ibcERC20Implementation = address(new IBCERC20());
        }

        if (deployment.escrowImplementation == address(0)) {
            deployment.escrowImplementation = address(new Escrow());
        }

        ERC1967Proxy transferProxy = deployProxiedICS20Transfer(deployment);
    
        IICS26Router ics26Router = IICS26Router(deployment.ics26Router);
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(transferProxy));

        vm.stopBroadcast();

        deployment.proxy = payable(address(transferProxy));
        verify(deployment);

        return address(transferProxy);
    }
}
