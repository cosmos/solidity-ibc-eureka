// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable gas-custom-errors

import { Test } from "forge-std/Test.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { IICS20Errors } from "../../contracts/errors/IICS20Errors.sol";
import { IIBCUUPSUpgradeableErrors } from "../../contracts/errors/IIBCUUPSUpgradeableErrors.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { DummyInitializable, ErroneousInitializable } from "./mocks/DummyInitializable.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

contract IBCUUPSUpgradeableTest is Test {
    ICS26Router public ics26Router;
    ICS20Transfer public ics20Transfer;

    address ics20Pauser = makeAddr("ics20Pauser");

    string public counterpartyId = "42-dummy-01";
    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function setUp() public {
        // ============ Step 1: Deploy the logic contracts ==============
        DummyLightClient lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        ICS26Router ics26RouterLogic = new ICS26Router();
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        // ============== Step 2: Deploy ERC1967 Proxies ==============
        ERC1967Proxy routerProxy = new ERC1967Proxy(
            address(ics26RouterLogic),
            abi.encodeWithSelector(ICS26Router.initialize.selector, address(this), address(this))
        );

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic), abi.encodeWithSelector(ICS20Transfer.initialize.selector, address(routerProxy), ics20Pauser)
        );

        // ============== Step 3: Wire up the contracts ==============
        ics26Router = ICS26Router(address(routerProxy));
        ics20Transfer = ICS20Transfer(address(transferProxy));

        ics26Router.addClient(
            "07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyId, merklePrefix), address(lightClient)
        );
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
}
