// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable gas-custom-errors

import { Test } from "forge-std/Test.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { TestERC20 } from "./mocks/TestERC20.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { DummyInitializable, ErroneousInitializable } from "./mocks/DummyInitializable.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";
import { ProxyAdmin } from "@openzeppelin-contracts/proxy/transparent/ProxyAdmin.sol";
import { IERC1967 } from "@openzeppelin-contracts/interfaces/IERC1967.sol";
import { VmSafe } from "forge-std/Vm.sol";

contract MigrationTest is Test {
    DummyLightClient public lightClient;
    ICS26Router public ics26Router;
    ICS20Transfer public ics20Transfer;
    TestERC20 public erc20;
    string public clientIdentifier;

    string public counterpartyId = "42-dummy-01";
    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function setUp() public {
        // ============ Step 1: Deploy the logic contracts ==============
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        ICS26Router ics26RouterLogic = new ICS26Router();
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        // ============== Step 2: Deploy Transparent Proxies ==============
        vm.recordLogs();

        ERC1967Proxy routerProxy = new ERC1967Proxy(
            address(ics26RouterLogic),
            abi.encodeWithSelector(ICS26Router.initialize.selector, address(this), address(this))
        );

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic),
            abi.encodeWithSelector(ICS20Transfer.initialize.selector, address(routerProxy))
        );

        // ============== Step 3: Wire up the contracts ==============
        ics26Router = ICS26Router(address(routerProxy));
        ics20Transfer = ICS20Transfer(address(transferProxy));
        erc20 = new TestERC20();

        clientIdentifier = ics26Router.addClient(
            "07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyId, merklePrefix), address(lightClient)
        );

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp(ICS20Lib.DEFAULT_PORT_ID)));
    }

    function test_success_upgrade() public {
        // ============== Step 4: Migrate the contracts ==============
        DummyInitializable newLogic = new DummyInitializable();

        UUPSUpgradeable(address(ics20Transfer)).upgradeToAndCall(
            address(newLogic),
            abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );
    }

    function test_failure_upgrade() public {
        // ============== Step 4: Migrate the contracts ==============
        ErroneousInitializable newLogic = new ErroneousInitializable();

        vm.expectRevert(abi.encodeWithSelector(ErroneousInitializable.InitializeFailed.selector));
        UUPSUpgradeable(address(ics20Transfer)).upgradeToAndCall(
            address(newLogic),
            abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );
    }
}
