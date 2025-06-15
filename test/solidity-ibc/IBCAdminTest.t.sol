// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable gas-custom-errors

import { Test } from "forge-std/Test.sol";

import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";

import { IAccessManaged } from "@openzeppelin-contracts/access/manager/IAccessManaged.sol";

import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { DummyInitializable, ErroneousInitializable } from "./mocks/DummyInitializable.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { PausableUpgradeable } from "@openzeppelin-upgradeable/utils/PausableUpgradeable.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../contracts/utils/Escrow.sol";
import { UpgradeableBeacon } from "@openzeppelin-contracts/proxy/beacon/UpgradeableBeacon.sol";
import { DeployAccessManagerWithRoles } from "../../scripts/deployments/DeployAccessManagerWithRoles.sol";
import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";
import { IBCRolesLib } from "../../contracts/utils/IBCRolesLib.sol";

contract IBCAdminTest is Test, DeployAccessManagerWithRoles {
    ICS26Router public ics26Router;
    ICS20Transfer public ics20Transfer;
    AccessManager public accessManager;

    address public customizer = makeAddr("customizer");
    address public ics20Pauser = makeAddr("ics20Pauser");
    address public ics20Unpauser = makeAddr("ics20Unpauser");
    address public relayer = makeAddr("relayer");
    address public tokenOperator = makeAddr("tokenOperator");
    address public erc20Customizer = makeAddr("erc20Customizer");

    string public clientId;
    string public counterpartyId = "42-dummy-01";
    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function setUp() public {
        // ============ Step 1: Deploy the logic contracts ==============
        DummyLightClient lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        ICS26Router ics26RouterLogic = new ICS26Router();
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        // ============== Step 2: Deploy ERC1967 Proxies ==============
        accessManager = new AccessManager(address(this));

        ERC1967Proxy routerProxy = new ERC1967Proxy(
            address(ics26RouterLogic), abi.encodeCall(ICS26Router.initialize, (address(accessManager)))
        );

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic),
            abi.encodeCall(
                ICS20Transfer.initialize,
                (address(routerProxy), escrowLogic, ibcERC20Logic, address(0), address(accessManager))
            )
        );

        // ============== Step 3: Wire up the contracts ==============
        ics26Router = ICS26Router(address(routerProxy));
        ics20Transfer = ICS20Transfer(address(transferProxy));

        accessManagerSetTargetRoles(accessManager, address(routerProxy), address(transferProxy), false);

        accessManager.grantRole(IBCRolesLib.RELAYER_ROLE, relayer, 0);
        accessManager.grantRole(IBCRolesLib.ID_CUSTOMIZER_ROLE, customizer, 0);
        accessManager.grantRole(IBCRolesLib.PAUSER_ROLE, ics20Pauser, 0);
        accessManager.grantRole(IBCRolesLib.UNPAUSER_ROLE, ics20Unpauser, 0);
        accessManager.grantRole(IBCRolesLib.ERC20_CUSTOMIZER_ROLE, erc20Customizer, 0);

        clientId =
            ics26Router.addClient(IICS02ClientMsgs.CounterpartyInfo(counterpartyId, merklePrefix), address(lightClient));

        vm.prank(customizer);
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));
    }

    function test_success_ics20_upgrade() public {
        // ============== Step 4: Migrate the contracts ==============
        DummyInitializable newLogic = new DummyInitializable();

        ics20Transfer.upgradeToAndCall(address(newLogic), abi.encodeCall(DummyInitializable.initializeV2, ()));
    }

    function test_failure_ics20_upgrade() public {
        // Case 1: Revert on failed initialization
        ErroneousInitializable erroneousLogic = new ErroneousInitializable();

        vm.expectRevert(abi.encodeWithSelector(ErroneousInitializable.InitializeFailed.selector));
        ics20Transfer.upgradeToAndCall(address(erroneousLogic), abi.encodeCall(DummyInitializable.initializeV2, ()));

        // Case 2: Revert on unauthorized upgrade
        DummyInitializable newLogic = new DummyInitializable();

        address unauthorized = makeAddr("unauthorized");
        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IAccessManaged.AccessManagedUnauthorized.selector, unauthorized));
        ics20Transfer.upgradeToAndCall(address(newLogic), abi.encodeCall(DummyInitializable.initializeV2, ()));
    }

    function test_success_ics26_upgrade() public {
        // ============== Step 4: Migrate the contracts ==============
        DummyInitializable newLogic = new DummyInitializable();

        ics26Router.upgradeToAndCall(address(newLogic), abi.encodeCall(DummyInitializable.initializeV2, ()));
    }

    function test_failure_ics26_upgrade() public {
        // Case 1: Revert on failed initialization
        ErroneousInitializable erroneousLogic = new ErroneousInitializable();

        vm.expectRevert(abi.encodeWithSelector(ErroneousInitializable.InitializeFailed.selector));
        ics26Router.upgradeToAndCall(address(erroneousLogic), abi.encodeCall(DummyInitializable.initializeV2, ()));

        // Case 2: Revert on unauthorized upgrade
        DummyInitializable newLogic = new DummyInitializable();

        address unauthorized = makeAddr("unauthorized");
        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IAccessManaged.AccessManagedUnauthorized.selector, unauthorized));
        ics26Router.upgradeToAndCall(address(newLogic), abi.encodeCall(DummyInitializable.initializeV2, ()));
    }

    function test_success_pauseAndUnpause() public {
        vm.prank(ics20Pauser);
        ics20Transfer.pause();
        assert(ics20Transfer.paused());

        // Try to call a paused function
        IICS20TransferMsgs.SendTransferMsg memory sendMsg;
        vm.expectRevert(abi.encodeWithSelector(PausableUpgradeable.EnforcedPause.selector));
        ics20Transfer.sendTransfer(sendMsg);

        vm.prank(ics20Unpauser);
        ics20Transfer.unpause();
        assert(!ics20Transfer.paused());
    }

    function test_failure_pauseAndUnpause() public {
        address unauthorized = makeAddr("unauthorized");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IAccessManaged.AccessManagedUnauthorized.selector, unauthorized));
        ics20Transfer.pause();
        assert(!ics20Transfer.paused());

        vm.prank(ics20Pauser);
        ics20Transfer.pause();
        assert(ics20Transfer.paused());

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IAccessManaged.AccessManagedUnauthorized.selector, unauthorized));
        ics20Transfer.unpause();
        assert(ics20Transfer.paused());

        vm.prank(ics20Pauser);
        vm.expectRevert(abi.encodeWithSelector(IAccessManaged.AccessManagedUnauthorized.selector, ics20Pauser));
        ics20Transfer.unpause();
        assert(ics20Transfer.paused());
    }

    function test_success_escrow_upgrade() public {
        DummyInitializable newLogic = new DummyInitializable();

        ics20Transfer.upgradeEscrowTo(address(newLogic));
        UpgradeableBeacon beacon = UpgradeableBeacon(ics20Transfer.getEscrowBeacon());
        assertEq(beacon.implementation(), address(newLogic));
    }

    function test_failure_escrow_upgrade() public {
        DummyInitializable newLogic = new DummyInitializable();
        address unauthorized = makeAddr("unauthorized");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IAccessManaged.AccessManagedUnauthorized.selector, unauthorized));
        ics20Transfer.upgradeEscrowTo(address(newLogic));
    }

    function test_success_ibcERC20_upgrade() public {
        DummyInitializable newLogic = new DummyInitializable();

        ics20Transfer.upgradeIBCERC20To(address(newLogic));
        UpgradeableBeacon beacon = UpgradeableBeacon(ics20Transfer.getIBCERC20Beacon());
        assertEq(beacon.implementation(), address(newLogic));
    }

    function test_failure_ibcERC20_upgrade() public {
        DummyInitializable newLogic = new DummyInitializable();
        address unauthorized = makeAddr("unauthorized");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IAccessManaged.AccessManagedUnauthorized.selector, unauthorized));
        ics20Transfer.upgradeIBCERC20To(address(newLogic));
    }
}
