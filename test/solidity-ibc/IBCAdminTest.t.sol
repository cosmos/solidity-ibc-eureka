// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable gas-custom-errors

import { Test } from "forge-std/Test.sol";

import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";

import { IICS20Errors } from "../../contracts/errors/IICS20Errors.sol";
import { IIBCUUPSUpgradeableErrors } from "../../contracts/errors/IIBCUUPSUpgradeableErrors.sol";
import { IAccessControl } from "@openzeppelin/contracts/access/IAccessControl.sol";

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

contract IBCAdminTest is Test {
    ICS26Router public ics26Router;
    ICS20Transfer public ics20Transfer;

    address public clientCreator = makeAddr("clientCreator");
    address public portCustomizer = makeAddr("portCustomizer");
    address public ics20Pauser = makeAddr("ics20Pauser");

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
        ERC1967Proxy routerProxy = new ERC1967Proxy(
            address(ics26RouterLogic), abi.encodeCall(ICS26Router.initialize, (address(this), portCustomizer))
        );

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic),
            abi.encodeCall(
                ICS20Transfer.initialize, (address(routerProxy), escrowLogic, ibcERC20Logic, ics20Pauser, address(0))
            )
        );

        // ============== Step 3: Wire up the contracts ==============
        ics26Router = ICS26Router(address(routerProxy));
        ics20Transfer = ICS20Transfer(address(transferProxy));

        vm.prank(clientCreator);
        clientId =
            ics26Router.addClient(IICS02ClientMsgs.CounterpartyInfo(counterpartyId, merklePrefix), address(lightClient));
        vm.prank(portCustomizer);
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));
    }

    function test_success_ics20_upgrade() public {
        // ============== Step 4: Migrate the contracts ==============
        DummyInitializable newLogic = new DummyInitializable();

        ics20Transfer.upgradeToAndCall(
            address(newLogic), abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );
    }

    function test_failure_ics20_upgrade() public {
        // Case 1: Revert on failed initialization
        ErroneousInitializable erroneousLogic = new ErroneousInitializable();

        vm.expectRevert(abi.encodeWithSelector(ErroneousInitializable.InitializeFailed.selector));
        ics20Transfer.upgradeToAndCall(
            address(erroneousLogic), abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );

        // Case 2: Revert on unauthorized upgrade
        DummyInitializable newLogic = new DummyInitializable();

        address unauthorized = makeAddr("unauthorized");
        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, unauthorized));
        ics20Transfer.upgradeToAndCall(
            address(newLogic), abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );
    }

    function test_success_ics26_upgrade() public {
        // ============== Step 4: Migrate the contracts ==============
        DummyInitializable newLogic = new DummyInitializable();

        ics26Router.upgradeToAndCall(
            address(newLogic), abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );
    }

    function test_failure_ics26_upgrade() public {
        // Case 1: Revert on failed initialization
        ErroneousInitializable erroneousLogic = new ErroneousInitializable();

        vm.expectRevert(abi.encodeWithSelector(ErroneousInitializable.InitializeFailed.selector));
        ics26Router.upgradeToAndCall(
            address(erroneousLogic), abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );

        // Case 2: Revert on unauthorized upgrade
        DummyInitializable newLogic = new DummyInitializable();

        address unauthorized = makeAddr("unauthorized");
        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IIBCUUPSUpgradeableErrors.Unauthorized.selector));
        ics26Router.upgradeToAndCall(
            address(newLogic), abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );
    }

    function test_success_setGovAdmin() public {
        address govAdmin = makeAddr("govAdmin");

        ics26Router.setGovAdmin(govAdmin);
        assertEq(ics26Router.getGovAdmin(), govAdmin);

        // Have the govAdmin change the govAdmin
        address newGovAdmin = makeAddr("newGovAdmin");
        ics26Router.setGovAdmin(newGovAdmin);
        assertEq(ics26Router.getGovAdmin(), newGovAdmin);
    }

    function test_failure_setGovAdmin() public {
        address unauthorized = makeAddr("unauthorized");
        address govAdmin = makeAddr("govAdmin");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IIBCUUPSUpgradeableErrors.Unauthorized.selector));
        ics26Router.setGovAdmin(govAdmin);
    }

    function test_success_setTimelockedAdmin() public {
        address newTimelockedAdmin = makeAddr("timelockedAdmin");

        ics26Router.setTimelockedAdmin(newTimelockedAdmin);
        assertEq(ics26Router.getTimelockedAdmin(), newTimelockedAdmin);
    }

    function test_failure_setTimelockedAdmin() public {
        address unauthorized = makeAddr("unauthorized");
        address newTimelockedAdmin = makeAddr("timelockedAdmin");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IIBCUUPSUpgradeableErrors.Unauthorized.selector));
        ics26Router.setTimelockedAdmin(newTimelockedAdmin);
    }

    function test_success_setPortCustomizer() public {
        address newPortCustomizer = makeAddr("newPortCustomizer");

        ics26Router.grantPortCustomizerRole(newPortCustomizer);
        assert(ics26Router.hasRole(ics26Router.PORT_CUSTOMIZER_ROLE(), newPortCustomizer));

        ics26Router.revokePortCustomizerRole(portCustomizer);
        assertFalse(ics26Router.hasRole(ics26Router.PORT_CUSTOMIZER_ROLE(), portCustomizer));
    }

    function test_failure_setPortCustomizer() public {
        address unauthorized = makeAddr("unauthorized");
        address newPortCustomizer = makeAddr("newPortCustomizer");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IIBCUUPSUpgradeableErrors.Unauthorized.selector));
        ics26Router.grantPortCustomizerRole(newPortCustomizer);
        assertFalse(ics26Router.hasRole(ics26Router.PORT_CUSTOMIZER_ROLE(), newPortCustomizer));

        // Revoke the port customizer role from an unauthorized account
        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IIBCUUPSUpgradeableErrors.Unauthorized.selector));
        ics26Router.revokePortCustomizerRole(portCustomizer);
        assert(ics26Router.hasRole(ics26Router.PORT_CUSTOMIZER_ROLE(), portCustomizer));

        // Check that an unauthorized account cannot set the port
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector,
                unauthorized,
                ics26Router.PORT_CUSTOMIZER_ROLE()
            )
        );
        vm.prank(unauthorized);
        ics26Router.addIBCApp("newPort", address(ics20Transfer));
    }

    function test_success_setClientMigrator() public {
        address newLightClientMigrator = makeAddr("newLightClientMigrator");

        ics26Router.grantLightClientMigratorRole(clientId, newLightClientMigrator);
        assert(ics26Router.hasRole(ics26Router.getLightClientMigratorRole(clientId), newLightClientMigrator));

        ics26Router.revokeLightClientMigratorRole(clientId, clientCreator);
        assertFalse(ics26Router.hasRole(ics26Router.getLightClientMigratorRole(clientId), clientCreator));
    }

    function test_failure_setClientMigrator() public {
        address unauthorized = makeAddr("unauthorized");
        address newLightClientMigrator = makeAddr("newLightClientMigrator");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IIBCUUPSUpgradeableErrors.Unauthorized.selector));
        ics26Router.grantLightClientMigratorRole(clientId, newLightClientMigrator);
        assertFalse(ics26Router.hasRole(ics26Router.getLightClientMigratorRole(clientId), newLightClientMigrator));

        // Revoke the light client migrator role from an unauthorized account
        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IIBCUUPSUpgradeableErrors.Unauthorized.selector));
        ics26Router.revokeLightClientMigratorRole(clientId, clientCreator);
        assert(ics26Router.hasRole(ics26Router.getLightClientMigratorRole(clientId), clientCreator));
    }

    function test_success_pauseAndUnpause() public {
        vm.prank(ics20Pauser);
        ics20Transfer.pause();
        assert(ics20Transfer.paused());

        // Try to call a paused function
        IICS20TransferMsgs.SendTransferMsg memory sendMsg;
        vm.expectRevert(abi.encodeWithSelector(PausableUpgradeable.EnforcedPause.selector));
        ics20Transfer.sendTransfer(sendMsg);

        vm.prank(ics20Pauser);
        ics20Transfer.unpause();
        assert(!ics20Transfer.paused());
    }

    function test_failure_pauseAndUnpause() public {
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, address(this), ics20Transfer.PAUSER_ROLE()
            )
        );
        ics20Transfer.pause();
        assert(!ics20Transfer.paused());

        vm.prank(ics20Pauser);
        ics20Transfer.pause();
        assert(ics20Transfer.paused());

        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, address(this), ics20Transfer.PAUSER_ROLE()
            )
        );
        ics20Transfer.unpause();
        assert(ics20Transfer.paused());
    }

    function test_success_setPauser() public {
        address newPauser = makeAddr("newPauser");

        ics20Transfer.grantPauserRole(newPauser);
        assertTrue(ics20Transfer.hasRole(ics20Transfer.PAUSER_ROLE(), newPauser));

        ics20Transfer.revokePauserRole(newPauser);
        assertFalse(ics20Transfer.hasRole(ics20Transfer.PAUSER_ROLE(), newPauser));
    }

    function test_failure_setPauser() public {
        address unauthorized = makeAddr("unauthorized");
        address newPauser = makeAddr("newPauser");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, unauthorized));
        ics20Transfer.grantPauserRole(newPauser);
        assertFalse(ics20Transfer.hasRole(ics20Transfer.PAUSER_ROLE(), newPauser));

        // Revoke the pauser role from an unauthorized account
        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, unauthorized));
        ics20Transfer.revokePauserRole(ics20Pauser);
    }

    function test_success_setDelegateSender() public {
        address delegateSender = makeAddr("delegateSender");

        ics20Transfer.grantDelegateSenderRole(delegateSender);
        assertTrue(ics20Transfer.hasRole(ics20Transfer.DELEGATE_SENDER_ROLE(), delegateSender));

        ics20Transfer.revokeDelegateSenderRole(delegateSender);
        assertFalse(ics20Transfer.hasRole(ics20Transfer.DELEGATE_SENDER_ROLE(), delegateSender));
    }

    function test_failure_setDelegateSender() public {
        address unauthorized = makeAddr("unauthorized");
        address delegateSender = makeAddr("delegateSender");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, unauthorized));
        ics20Transfer.grantDelegateSenderRole(delegateSender);
        assertFalse(ics20Transfer.hasRole(ics20Transfer.DELEGATE_SENDER_ROLE(), delegateSender));

        // Revoke the delegate sender role from an unauthorized account
        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, unauthorized));
        ics20Transfer.revokeDelegateSenderRole(delegateSender);
    }

    function test_failure_sendTransferWithSender() public {
        address sender = makeAddr("sender");
        IICS20TransferMsgs.SendTransferMsg memory msg_;

        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector,
                address(this),
                ics20Transfer.DELEGATE_SENDER_ROLE()
            )
        );
        ics20Transfer.sendTransferWithSender(msg_, sender);
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
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, unauthorized));
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
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, unauthorized));
        ics20Transfer.upgradeIBCERC20To(address(newLogic));
    }
}
