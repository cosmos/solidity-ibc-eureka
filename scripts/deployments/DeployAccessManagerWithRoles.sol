// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IBCRolesLib } from "../../contracts/utils/IBCRolesLib.sol";
import { IAccessManager } from "@openzeppelin-contracts/access/manager/IAccessManager.sol";
import { IICS26RouterAccessControlled } from "../../contracts/interfaces/IICS26Router.sol";
import { IICS20TransferAccessControlled } from "../../contracts/interfaces/IICS20Transfer.sol";
import { IICS02ClientAccessControlled } from "../../contracts/interfaces/IICS02Client.sol";
import { IIBCPausable } from "../../contracts/interfaces/IIBCPausable.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";

abstract contract DeployAccessManagerWithRoles {
    bytes4[] public ics26IdCustomizerFunctions =
        [IICS26RouterAccessControlled.addIBCApp.selector, IICS02ClientAccessControlled.addClient.selector];
    bytes4[] public ics26RelayerFunctions = [
        IICS26RouterAccessControlled.recvPacket.selector,
        IICS26RouterAccessControlled.timeoutPacket.selector,
        IICS26RouterAccessControlled.ackPacket.selector,
        IICS02ClientAccessControlled.updateClient.selector
    ];
    bytes4[] public pauserFunctions = [IIBCPausable.pause.selector];
    bytes4[] public unpauserFunctions = [IIBCPausable.unpause.selector];
    bytes4[] public erc20CustomizerFunctions = [IICS20TransferAccessControlled.setCustomERC20.selector];
    bytes4[] public delegateSenderFunctions = [IICS20TransferAccessControlled.sendTransferWithSender.selector];
    /**
     * @notice The upgrade functions are not restricted to a specific role.
     * Therefore they can only be called by the `ADMIN_ROLE`.
     */
    bytes4[] public upgradeFunctions = [UUPSUpgradeable.upgradeToAndCall.selector];
    /**
     * @notice The upgrade functions are not restricted to a specific role.
     * Therefore they can only be called by the `ADMIN_ROLE`.
     */
    bytes4[] public beaconUpgradeFunctions = [
        IICS20TransferAccessControlled.upgradeEscrowTo.selector,
        IICS20TransferAccessControlled.upgradeIBCERC20To.selector
    ];

    function accessManagerSetTargetRoles(
        IAccessManager accessManager,
        address ics26,
        address ics20,
        bool pubRelay
    )
        public
    {
        accessManager.setTargetFunctionRole(ics26, ics26IdCustomizerFunctions, IBCRolesLib.ID_CUSTOMIZER_ROLE);
        accessManager.setTargetFunctionRole(ics26, ics26RelayerFunctions, IBCRolesLib.RELAYER_ROLE);
        accessManager.setTargetFunctionRole(ics20, pauserFunctions, IBCRolesLib.PAUSER_ROLE);
        accessManager.setTargetFunctionRole(ics20, unpauserFunctions, IBCRolesLib.UNPAUSER_ROLE);
        accessManager.setTargetFunctionRole(ics20, erc20CustomizerFunctions, IBCRolesLib.ERC20_CUSTOMIZER_ROLE);
        accessManager.setTargetFunctionRole(ics20, delegateSenderFunctions, IBCRolesLib.DELEGATE_SENDER_ROLE);
        // TODO: fix rate limiter role

        if (pubRelay) {
            accessManager.setTargetFunctionRole(ics26, ics26RelayerFunctions, IBCRolesLib.PUBLIC_ROLE);
        }
    }

    function accessManagerSetRoles(
        IAccessManager accessManager,
        address ibcAdminContract,
        address[] memory relayers,
        address[] memory pausers,
        address[] memory unpausers,
        address idCustomizer,
        address erc20Customizer,
        address delegateSender
    )
        public
    {
        require(ibcAdminContract != address(0), "IBC Admin contract cannot be zero");

        for (uint256 i = 0; i < relayers.length; i++) {
            accessManager.grantRole(IBCRolesLib.RELAYER_ROLE, relayers[i], 0);
        }
        for (uint256 i = 0; i < pausers.length; i++) {
            accessManager.grantRole(IBCRolesLib.PAUSER_ROLE, pausers[i], 0);
        }
        for (uint256 i = 0; i < unpausers.length; i++) {
            accessManager.grantRole(IBCRolesLib.UNPAUSER_ROLE, unpausers[i], 0);
        }
        accessManager.grantRole(IBCRolesLib.ADMIN_ROLE, ibcAdminContract, 0);
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
