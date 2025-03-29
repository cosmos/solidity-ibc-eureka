// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

// solhint-disable-next-line no-global-import
import "forge-std/console.sol";

import { Deployments } from "../helpers/Deployments.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { IBCNFT721 } from "../../contracts/memes/IBCNFT721.sol";
import { IIBCUUPSUpgradeable } from "../../contracts/interfaces/IIBCUUPSUpgradeable.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { ERC1967Utils } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Utils.sol";
import { Script } from "forge-std/Script.sol";

abstract contract DeployProxiedMemeNFT721 is Deployments {
    function deployProxiedMemeNFT721(Deployments.ProxiedMemeDeployment memory deployment) public returns (ERC1967Proxy) {
        ERC1967Proxy memeNFT721Proxy = new ERC1967Proxy(
            deployment.implementation,
            abi.encodeCall(
                IBCNFT721.initialize, 
                (
                    deployment.ics26Router,
                    deployment.ics20,
                    deployment.name,
                    deployment.symbol
                )
            )
        );

        console.log("Deployed IBCNFT721 at address: ", address(memeNFT721Proxy));

        IBCNFT721 ibcNFT721 = IBCNFT721(address(memeNFT721Proxy));

        return memeNFT721Proxy;
    }
}

contract deployProxiedMemeNFT721Script is DeployProxiedMemeNFT721, Script {
    function getImplementation(address proxy) internal view returns (address) {
        return address(uint160(uint256(vm.load(proxy, ERC1967Utils.IMPLEMENTATION_SLOT))));
    }

    function verify(ProxiedMemeDeployment memory deployment) internal view {
        ERC1967Proxy memeProxy = ERC1967Proxy(payable(deployment.proxy));

        vm.assertEq(
            getImplementation(address(memeProxy)),
            deployment.implementation,
            "implementation addresses don't match"
        );

        IBCNFT721 ibcNFT721 = IBCNFT721(deployment.proxy);

        vm.assertEq(
            ibcNFT721.name(),
            deployment.name,
            "name addresses don't match"
        );

        vm.assertEq(
            ibcNFT721.symbol(),
            deployment.symbol,
            "symbol addresses don't match"
        );

        vm.assertEq(
            ibcNFT721.ics20(),
            deployment.ics20,
            "ics20 addresses don't match"
        );

        vm.assertEq(
            ibcNFT721.ics26(),
            deployment.ics26Router,
            "ics26 addresses don't match"
        );

    }
}
