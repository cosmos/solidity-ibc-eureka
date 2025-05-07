// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

// solhint-disable-next-line no-global-import
import "forge-std/console.sol";

import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";

abstract contract DeployProxiedICS20Transfer {
    function deployProxiedICS20Transfer(
        address implementation,
        address ics26Router,
        address escrowImplementation,
        address ibcERC20Implementation,
        address[] memory pausers,
        address[] memory unpausers,
        address tokenOperator,
        address permit2
    ) public returns (ERC1967Proxy) {
        ERC1967Proxy transferProxy = new ERC1967Proxy(
            implementation,
            abi.encodeCall(
                ICS20Transfer.initialize,
                (
                    ics26Router,
                    escrowImplementation,
                    ibcERC20Implementation,
                    permit2
                )
            )
        );

        console.log("Deployed ICS20Transfer at address: ", address(transferProxy));

        ICS20Transfer ics20Transfer = ICS20Transfer(address(transferProxy));

        for (uint32 i = 0; i < pausers.length; i++) {
            address pauser = pausers[i];
            console.log("Granting pauser role to: ", pauser);
            ics20Transfer.grantPauserRole(pauser);
        }

        for (uint32 i = 0; i < unpausers.length; i++) {
            address unpauser = unpausers[i];
            console.log("Granting unpauser role to: ", unpauser);
            ics20Transfer.grantUnpauserRole(unpauser);
        }

        if (tokenOperator != address(0)) {
            console.log("Granting tokenOperator role to: ", tokenOperator);
            ics20Transfer.grantTokenOperatorRole(tokenOperator);
        }

        return transferProxy;
    }
}
