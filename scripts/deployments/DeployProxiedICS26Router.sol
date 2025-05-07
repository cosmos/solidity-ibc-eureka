// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

abstract contract DeployProxiedICS26Router {
    function deployProxiedICS26Router(
        address implementation,
        address timeLockAdmin,
        address portCustomizer,
        address clientIdCustomizer,
        address[] memory relayers
    ) public returns (ERC1967Proxy) {
        require(msg.sender == timeLockAdmin, "sender must be timeLockAdmin");

        ERC1967Proxy routerProxy = new ERC1967Proxy(
            implementation,
            abi.encodeCall(ICS26Router.initialize, (timeLockAdmin))
        );

        ICS26Router ics26Router = ICS26Router(address(routerProxy));

        for (uint256 i = 0; i < relayers.length; i++) {
            ics26Router.grantRole(ics26Router.RELAYER_ROLE(), relayers[i]);
        }
         ics26Router.grantRole(ics26Router.PORT_CUSTOMIZER_ROLE(), portCustomizer);
         ics26Router.grantRole(ics26Router.CLIENT_ID_CUSTOMIZER_ROLE(), clientIdCustomizer);

        return routerProxy;
    }
}
