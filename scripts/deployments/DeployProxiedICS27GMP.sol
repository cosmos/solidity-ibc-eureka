// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

// solhint-disable-next-line no-global-import
import "forge-std/console.sol";

import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { ICS27GMP } from "../../contracts/ICS27GMP.sol";

abstract contract DeployProxiedICS27GMP {
    function deployProxiedICS27GMP(
        address implementation,
        address ics26Router,
        address accountImplementation
    )
        public
        returns (ERC1967Proxy)
    {
        ERC1967Proxy gmpProxy =
            new ERC1967Proxy(implementation, abi.encodeCall(ICS27GMP.initialize, (ics26Router, accountImplementation)));

        console.log("Deployed ICS27GMP at address: ", address(gmpProxy));

        return gmpProxy;
    }
}
