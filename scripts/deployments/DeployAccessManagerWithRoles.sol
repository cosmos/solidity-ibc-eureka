// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable gas-custom-errors,reason-string

import { IBCRolesLib } from "../../contracts/utils/IBCRolesLib.sol";
import { IAccessManager } from "@openzeppelin-contracts/access/manager/IAccessManager.sol";

abstract contract DeployAccessManagerWithRoles {
    function accessManagerSetTargetRoles(
        IAccessManager accessManager,
        address ics26,
        address ics20,
        bool pubRelay
    )
        public
    {
        accessManager.setTargetFunctionRole(
            ics26, IBCRolesLib.ics26IdCustomizerSelectors(), IBCRolesLib.ID_CUSTOMIZER_ROLE
        );
        accessManager.setTargetFunctionRole(ics26, IBCRolesLib.ics26RelayerSelectors(), IBCRolesLib.RELAYER_ROLE);
        accessManager.setTargetFunctionRole(ics20, IBCRolesLib.pauserSelectors(), IBCRolesLib.PAUSER_ROLE);
        accessManager.setTargetFunctionRole(ics20, IBCRolesLib.unpauserSelectors(), IBCRolesLib.UNPAUSER_ROLE);
        accessManager.setTargetFunctionRole(
            ics20, IBCRolesLib.erc20CustomizerSelectors(), IBCRolesLib.ERC20_CUSTOMIZER_ROLE
        );
        accessManager.setTargetFunctionRole(
            ics20, IBCRolesLib.delegateSenderSelectors(), IBCRolesLib.DELEGATE_SENDER_ROLE
        );
        // TODO: fix rate limiter role (#559)

        // Add admin role for upgradeable contracts
        // This is actually a no-op since if no role is set, the admin role is assumed
        accessManager.setTargetFunctionRole(ics20, IBCRolesLib.beaconUpgradeSelectors(), IBCRolesLib.ADMIN_ROLE);
        accessManager.setTargetFunctionRole(ics20, IBCRolesLib.uupsUpgradeSelectors(), IBCRolesLib.ADMIN_ROLE);
        accessManager.setTargetFunctionRole(ics26, IBCRolesLib.uupsUpgradeSelectors(), IBCRolesLib.ADMIN_ROLE);

        if (pubRelay) {
            accessManager.setTargetFunctionRole(ics26, IBCRolesLib.ics26RelayerSelectors(), IBCRolesLib.PUBLIC_ROLE);
        }
    }

    function accessManagerSetRoles(
        IAccessManager accessManager,
        address[] memory relayers,
        address[] memory pausers,
        address[] memory unpausers,
        address idCustomizer,
        address erc20Customizer,
        address delegateSender
    )
        public
    {
        for (uint256 i = 0; i < relayers.length; i++) {
            accessManager.grantRole(IBCRolesLib.RELAYER_ROLE, relayers[i], 0);
        }
        for (uint256 i = 0; i < pausers.length; i++) {
            accessManager.grantRole(IBCRolesLib.PAUSER_ROLE, pausers[i], 0);
        }
        for (uint256 i = 0; i < unpausers.length; i++) {
            accessManager.grantRole(IBCRolesLib.UNPAUSER_ROLE, unpausers[i], 0);
        }
        if (idCustomizer != address(0)) {
            accessManager.grantRole(IBCRolesLib.ID_CUSTOMIZER_ROLE, idCustomizer, 0);
        }
        if (erc20Customizer != address(0)) {
            accessManager.grantRole(IBCRolesLib.ERC20_CUSTOMIZER_ROLE, erc20Customizer, 0);
        }
        if (delegateSender != address(0)) {
            accessManager.grantRole(IBCRolesLib.DELEGATE_SENDER_ROLE, delegateSender, 0);
        }
    }
}
