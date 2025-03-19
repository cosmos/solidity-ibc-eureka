// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable gas-custom-errors

import { Test } from "forge-std/Test.sol";

import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";

import { IICS20Errors } from "../../contracts/errors/IICS20Errors.sol";
import { IIBCUUPSUpgradeableErrors } from "../../contracts/errors/IIBCUUPSUpgradeableErrors.sol";
import { IICS26RouterErrors } from "../../contracts/errors/IICS26RouterErrors.sol";
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
    address public customizer = makeAddr("customizer");
    address public ics20Pauser = makeAddr("ics20Pauser");
    address public ics20Unpauser = makeAddr("ics20Unpauser");
    address public relayer = makeAddr("relayer");

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
        ERC1967Proxy routerProxy =
            new ERC1967Proxy(address(ics26RouterLogic), abi.encodeCall(ICS26Router.initialize, (address(this))));

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic),
            abi.encodeCall(
                ICS20Transfer.initialize,
                (address(routerProxy), escrowLogic, ibcERC20Logic, ics20Pauser, ics20Unpauser, address(0))
            )
        );

        // ============== Step 3: Wire up the contracts ==============
        ics26Router = ICS26Router(address(routerProxy));
        ics20Transfer = ICS20Transfer(address(transferProxy));

        ics26Router.grantRole(ics26Router.RELAYER_ROLE(), relayer);
        ics26Router.grantRole(ics26Router.PORT_CUSTOMIZER_ROLE(), customizer);
        ics26Router.grantRole(ics26Router.CLIENT_ID_CUSTOMIZER_ROLE(), customizer);

        vm.prank(clientCreator);
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
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, unauthorized));
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
        vm.expectRevert(abi.encodeWithSelector(IIBCUUPSUpgradeableErrors.Unauthorized.selector));
        ics26Router.upgradeToAndCall(address(newLogic), abi.encodeCall(DummyInitializable.initializeV2, ()));
    }

    function test_success_setGovAdmin() public {
        address govAdmin = makeAddr("govAdmin");

        ics26Router.setGovAdmin(govAdmin);
        assertEq(ics26Router.getGovAdmin(), govAdmin);
        assert(ics26Router.hasRole(ics26Router.DEFAULT_ADMIN_ROLE(), govAdmin));

        // Have the govAdmin change the govAdmin
        address newGovAdmin = makeAddr("newGovAdmin");
        ics26Router.setGovAdmin(newGovAdmin);
        assertEq(ics26Router.getGovAdmin(), newGovAdmin);
        assertFalse(ics26Router.hasRole(ics26Router.DEFAULT_ADMIN_ROLE(), govAdmin));
        assert(ics26Router.hasRole(ics26Router.DEFAULT_ADMIN_ROLE(), newGovAdmin));
    }

    function test_failure_setGovAdmin() public {
        address unauthorized = makeAddr("unauthorized");
        address govAdmin = makeAddr("govAdmin");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IIBCUUPSUpgradeableErrors.Unauthorized.selector));
        ics26Router.setGovAdmin(govAdmin);
        assertFalse(ics26Router.hasRole(ics26Router.DEFAULT_ADMIN_ROLE(), govAdmin));
    }

    function test_success_setTimelockedAdmin() public {
        assert(ics26Router.hasRole(ics26Router.DEFAULT_ADMIN_ROLE(), address(this)));
        address newTimelockedAdmin = makeAddr("newTimelockedAdmin");

        ics26Router.setTimelockedAdmin(newTimelockedAdmin);
        assertEq(ics26Router.getTimelockedAdmin(), newTimelockedAdmin);
        assertFalse(ics26Router.hasRole(ics26Router.DEFAULT_ADMIN_ROLE(), address(this)));
        assert(ics26Router.hasRole(ics26Router.DEFAULT_ADMIN_ROLE(), newTimelockedAdmin));
    }

    function test_failure_setTimelockedAdmin() public {
        address unauthorized = makeAddr("unauthorized");
        address newTimelockedAdmin = makeAddr("newTimelockedAdmin");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IIBCUUPSUpgradeableErrors.Unauthorized.selector));
        ics26Router.setTimelockedAdmin(newTimelockedAdmin);
        assertFalse(ics26Router.hasRole(ics26Router.DEFAULT_ADMIN_ROLE(), newTimelockedAdmin));
        assert(ics26Router.hasRole(ics26Router.DEFAULT_ADMIN_ROLE(), address(this)));
    }

    function test_failure_grantDefaultAdminRole() public {
        bytes32 defaultAdminRole = ics26Router.DEFAULT_ADMIN_ROLE();
        address anyAddress = makeAddr("anyAddress");

        vm.expectRevert(abi.encodeWithSelector(IICS26RouterErrors.DefaultAdminRoleCannotBeGranted.selector));
        ics26Router.grantRole(defaultAdminRole, anyAddress);
        assertFalse(ics26Router.hasRole(defaultAdminRole, anyAddress));
    }

    function test_success_setPortCustomizer() public {
        bytes32 portCustomizerRole = ics26Router.PORT_CUSTOMIZER_ROLE();
        address newPortCustomizer = makeAddr("newPortCustomizer");

        ics26Router.grantRole(portCustomizerRole, newPortCustomizer);
        assert(ics26Router.hasRole(portCustomizerRole, newPortCustomizer));

        ics26Router.revokeRole(portCustomizerRole, customizer);
        assertFalse(ics26Router.hasRole(portCustomizerRole, customizer));
    }

    function test_failure_setPortCustomizer() public {
        bytes32 defaultAdminRole = ics26Router.DEFAULT_ADMIN_ROLE();
        bytes32 portCustomizerRole = ics26Router.PORT_CUSTOMIZER_ROLE();
        address unauthorized = makeAddr("unauthorized");
        address newPortCustomizer = makeAddr("newPortCustomizer");

        vm.prank(unauthorized);
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, defaultAdminRole
            )
        );
        ics26Router.grantRole(portCustomizerRole, newPortCustomizer);
        assertFalse(ics26Router.hasRole(portCustomizerRole, newPortCustomizer));

        // Revoke the port customizer role from an unauthorized account
        vm.prank(unauthorized);
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, defaultAdminRole
            )
        );
        ics26Router.revokeRole(portCustomizerRole, customizer);
        assert(ics26Router.hasRole(portCustomizerRole, customizer));

        // Check that an unauthorized account cannot set the port
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, portCustomizerRole
            )
        );
        vm.prank(unauthorized);
        ics26Router.addIBCApp("newPort", address(ics20Transfer));
    }

    function test_success_setClientIdCustomizer() public {
        bytes32 clientIdCustomizerRole = ics26Router.CLIENT_ID_CUSTOMIZER_ROLE();
        address newCustomizer = makeAddr("newCustomizer");

        ics26Router.grantRole(clientIdCustomizerRole, newCustomizer);
        assert(ics26Router.hasRole(clientIdCustomizerRole, newCustomizer));

        ics26Router.revokeRole(clientIdCustomizerRole, customizer);
        assertFalse(ics26Router.hasRole(clientIdCustomizerRole, customizer));
    }

    function test_failure_setClientIdCustomizer() public {
        bytes32 defaultAdminRole = ics26Router.DEFAULT_ADMIN_ROLE();
        bytes32 clientIdCustomizerRole = ics26Router.CLIENT_ID_CUSTOMIZER_ROLE();
        address unauthorized = makeAddr("unauthorized");
        address newCustomizer = makeAddr("newCustomizer");

        vm.prank(unauthorized);
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, defaultAdminRole
            )
        );
        ics26Router.grantRole(clientIdCustomizerRole, newCustomizer);
        assertFalse(ics26Router.hasRole(clientIdCustomizerRole, newCustomizer));

        // Revoke the port customizer role from an unauthorized account
        vm.prank(unauthorized);
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, defaultAdminRole
            )
        );
        ics26Router.revokeRole(clientIdCustomizerRole, customizer);
        assert(ics26Router.hasRole(clientIdCustomizerRole, customizer));

        // Check that an unauthorized account cannot set the port
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, clientIdCustomizerRole
            )
        );
        vm.prank(unauthorized);
        ics26Router.addClient("newClient", IICS02ClientMsgs.CounterpartyInfo(counterpartyId, merklePrefix), address(0));
    }

    function test_success_setRelayer() public {
        bytes32 relayerRole = ics26Router.RELAYER_ROLE();
        address newRelayer = makeAddr("newRelayer");

        ics26Router.grantRole(relayerRole, newRelayer);
        assert(ics26Router.hasRole(relayerRole, newRelayer));

        ics26Router.revokeRole(relayerRole, newRelayer);
        assertFalse(ics26Router.hasRole(relayerRole, newRelayer));
    }

    function test_failure_setRelayer() public {
        bytes32 defaultAdminRole = ics26Router.DEFAULT_ADMIN_ROLE();
        bytes32 relayerRole = ics26Router.RELAYER_ROLE();
        address unauthorized = makeAddr("unauthorized");
        address newRelayer = makeAddr("newRelayer");

        vm.prank(unauthorized);
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, defaultAdminRole
            )
        );
        ics26Router.grantRole(relayerRole, newRelayer);
        assertFalse(ics26Router.hasRole(relayerRole, newRelayer));

        // Revoke the port customizer role from an unauthorized account
        vm.prank(unauthorized);
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, defaultAdminRole
            )
        );
        ics26Router.revokeRole(relayerRole, relayer);
        assert(ics26Router.hasRole(relayerRole, relayer));

        // Check that an unauthorized account cannot call recvPacket
        IICS26RouterMsgs.MsgRecvPacket memory recvMsg;
        vm.expectRevert(
            abi.encodeWithSelector(IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, relayerRole)
        );
        vm.prank(unauthorized);
        ics26Router.recvPacket(recvMsg);

        // Check that an unauthorized account cannot call timeoutPacket
        IICS26RouterMsgs.MsgTimeoutPacket memory timeoutMsg;
        vm.expectRevert(
            abi.encodeWithSelector(IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, relayerRole)
        );
        vm.prank(unauthorized);
        ics26Router.timeoutPacket(timeoutMsg);

        // Check that an unauthorized account cannot call ackPacket
        IICS26RouterMsgs.MsgAckPacket memory ackMsg;
        vm.expectRevert(
            abi.encodeWithSelector(IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, relayerRole)
        );
        vm.prank(unauthorized);
        ics26Router.ackPacket(ackMsg);
    }

    function test_success_setClientMigrator() public {
        bytes32 clientMigratorRole = ics26Router.getLightClientMigratorRole(clientId);
        address newLightClientMigrator = makeAddr("newLightClientMigrator");

        ics26Router.grantRole(clientMigratorRole, newLightClientMigrator);
        assert(ics26Router.hasRole(clientMigratorRole, newLightClientMigrator));

        ics26Router.revokeRole(clientMigratorRole, clientCreator);
        assertFalse(ics26Router.hasRole(clientMigratorRole, clientCreator));
    }

    function test_failure_setClientMigrator() public {
        bytes32 defaultAdminRole = ics26Router.DEFAULT_ADMIN_ROLE();
        bytes32 clientMigratorRole = ics26Router.getLightClientMigratorRole(clientId);
        address unauthorized = makeAddr("unauthorized");
        address newLightClientMigrator = makeAddr("newLightClientMigrator");

        vm.prank(unauthorized);
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, defaultAdminRole
            )
        );
        ics26Router.grantRole(clientMigratorRole, newLightClientMigrator);
        assertFalse(ics26Router.hasRole(clientMigratorRole, newLightClientMigrator));

        // Revoke the light client migrator role from an unauthorized account
        vm.prank(unauthorized);
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, defaultAdminRole
            )
        );
        ics26Router.revokeRole(clientMigratorRole, clientCreator);
        assert(ics26Router.hasRole(clientMigratorRole, clientCreator));
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
                IAccessControl.AccessControlUnauthorizedAccount.selector, address(this), ics20Transfer.UNPAUSER_ROLE()
            )
        );
        ics20Transfer.unpause();
        assert(ics20Transfer.paused());

        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, ics20Pauser, ics20Transfer.UNPAUSER_ROLE()
            )
        );
        vm.prank(ics20Pauser);
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
        assertFalse(ics20Transfer.hasRole(ics20Transfer.UNPAUSER_ROLE(), newPauser));

        // Revoke the pauser role from an unauthorized account
        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, unauthorized));
        ics20Transfer.revokePauserRole(ics20Pauser);
    }

    function test_success_setUnpauser() public {
        address newUnpauser = makeAddr("newUnpauser");

        ics20Transfer.grantUnpauserRole(newUnpauser);
        assertTrue(ics20Transfer.hasRole(ics20Transfer.UNPAUSER_ROLE(), newUnpauser));

        ics20Transfer.revokeUnpauserRole(newUnpauser);
        assertFalse(ics20Transfer.hasRole(ics20Transfer.UNPAUSER_ROLE(), newUnpauser));
    }

    function test_failure_setUnpauser() public {
        address unauthorized = makeAddr("unauthorized");
        address newUnpauser = makeAddr("newUnpauser");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, unauthorized));
        ics20Transfer.grantUnpauserRole(newUnpauser);
        assertFalse(ics20Transfer.hasRole(ics20Transfer.PAUSER_ROLE(), newUnpauser));

        // Revoke the pauser role from an unauthorized account
        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, unauthorized));
        ics20Transfer.revokeUnpauserRole(ics20Unpauser);
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
