// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable gas-custom-errors

import { Test } from "forge-std/Test.sol";

import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";

import { IICS20Errors } from "../../contracts/errors/IICS20Errors.sol";
import { IIBCUUPSUpgradeableErrors } from "../../contracts/errors/IIBCUUPSUpgradeableErrors.sol";
import { IIBCPausableUpgradeableErrors } from "../../contracts/errors/IIBCPausableUpgradeableErrors.sol";

import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { DummyInitializable, ErroneousInitializable } from "./mocks/DummyInitializable.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { PausableUpgradeable } from "@openzeppelin-upgradeable/utils/PausableUpgradeable.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../contracts/utils/Escrow.sol";

contract IBCAdminTest is Test {
    ICS26Router public ics26Router;
    ICS20Transfer public ics20Transfer;

    address public ics20Pauser = makeAddr("ics20Pauser");

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
            address(ics26RouterLogic),
            abi.encodeWithSelector(ICS26Router.initialize.selector, address(this), address(this))
        );

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic),
            abi.encodeWithSelector(
                ICS20Transfer.initialize.selector, address(routerProxy), escrowLogic, ibcERC20Logic, ics20Pauser
            )
        );

        // ============== Step 3: Wire up the contracts ==============
        ics26Router = ICS26Router(address(routerProxy));
        ics20Transfer = ICS20Transfer(address(transferProxy));

        ics26Router.addClient(IICS02ClientMsgs.CounterpartyInfo(counterpartyId, merklePrefix), address(lightClient));
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

    function test_success_pauseAndUnpause() public {
        assertEq(ics20Transfer.getPauser(), ics20Pauser);

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
        vm.expectRevert(abi.encodeWithSelector(IIBCPausableUpgradeableErrors.Unauthorized.selector));
        ics20Transfer.pause();
        assert(!ics20Transfer.paused());

        vm.prank(ics20Pauser);
        ics20Transfer.pause();
        assert(ics20Transfer.paused());

        vm.expectRevert(abi.encodeWithSelector(IIBCPausableUpgradeableErrors.Unauthorized.selector));
        ics20Transfer.unpause();
        assert(ics20Transfer.paused());
    }

    function test_success_setPauser() public {
        address newPauser = makeAddr("newPauser");

        ics20Transfer.setPauser(newPauser);
        assertEq(ics20Transfer.getPauser(), newPauser);
    }

    function test_failure_setPauser() public {
        address unauthorized = makeAddr("unauthorized");
        address newPauser = makeAddr("newPauser");

        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, unauthorized));
        ics20Transfer.setPauser(newPauser);
        assertEq(ics20Transfer.getPauser(), ics20Pauser);
    }

    function test_success_escrow_upgrade() public {
        DummyInitializable newLogic = new DummyInitializable();
        Escrow escrow = Escrow(ics20Transfer.escrow());

        escrow.upgradeToAndCall(
            address(newLogic), abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );
    }

    function test_failure_escrow_upgrade() public {
        DummyInitializable newLogic = new DummyInitializable();
        Escrow escrow = Escrow(ics20Transfer.escrow());

        address unauthorized = makeAddr("unauthorized");
        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(Escrow.EscrowUnauthorized.selector, unauthorized));
        escrow.upgradeToAndCall(
            address(newLogic), abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );
    }

    function test_success_ibcERC20_upgrade() public {
        DummyInitializable newLogic = new DummyInitializable();
        
        address ibcERC20Logic = address(new IBCERC20());
        IBCERC20 ibcERC20Proxy = IBCERC20(address(new ERC1967Proxy(
            ibcERC20Logic,
            abi.encodeWithSelector(IBCERC20.initialize.selector, address(ics20Transfer), address(ics20Transfer.escrow()), address(ics26Router), "test", "full/denom/path/test")
        )));

        ibcERC20Proxy.upgradeToAndCall(
            address(newLogic), abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );
    }

    function test_failure_ibcERC20_upgrade() public {
        DummyInitializable newLogic = new DummyInitializable();
        
        address ibcERC20Logic = address(new IBCERC20());
        IBCERC20 ibcERC20Proxy = IBCERC20(address(new ERC1967Proxy(
            ibcERC20Logic,
            abi.encodeWithSelector(IBCERC20.initialize.selector, address(ics20Transfer), address(ics20Transfer.escrow()), address(ics26Router), "test", "full/denom/path/test")
        )));

        address unauthorized = makeAddr("unauthorized");
        vm.prank(unauthorized);
        vm.expectRevert(abi.encodeWithSelector(IBCERC20.IBCERC20Unauthorized.selector, unauthorized));
        ibcERC20Proxy.upgradeToAndCall(
            address(newLogic), abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );
    }
}
