// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Test } from "forge-std/Test.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { IICS04ChannelMsgs } from "../../contracts/msgs/IICS04ChannelMsgs.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICSCore } from "../../contracts/ICSCore.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { TestERC20 } from "./mocks/TestERC20.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { DummyInitializable, ErroneousInitializable } from "./mocks/DummyInitializable.sol";
import {
    TransparentUpgradeableProxy,
    ITransparentUpgradeableProxy
} from "@openzeppelin/proxy/transparent/TransparentUpgradeableProxy.sol";
import { ProxyAdmin } from "@openzeppelin/proxy/transparent/ProxyAdmin.sol";
import { IERC1967 } from "@openzeppelin/interfaces/IERC1967.sol";
import { VmSafe } from "forge-std/Vm.sol";

contract MigrationTest is Test {
    DummyLightClient public lightClient;
    ICS26Router public ics26Router;
    ICS20Transfer public ics20Transfer;
    TestERC20 public erc20;
    string public clientIdentifier;
    ProxyAdmin public transferProxyAdmin;

    string public counterpartyId = "42-dummy-01";
    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function setUp() public {
        // ============ Step 1: Deploy the logic contracts ==============
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        ICSCore icsCoreLogic = new ICSCore();
        ICS26Router ics26RouterLogic = new ICS26Router();
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        // ============== Step 2: Deploy Transparent Proxies ==============
        vm.recordLogs();

        TransparentUpgradeableProxy coreProxy = new TransparentUpgradeableProxy(
            address(icsCoreLogic), address(this), abi.encodeWithSelector(ICSCore.initialize.selector, address(this))
        );

        TransparentUpgradeableProxy routerProxy = new TransparentUpgradeableProxy(
            address(ics26RouterLogic),
            address(this),
            abi.encodeWithSelector(ICS26Router.initialize.selector, address(this), address(coreProxy))
        );

        TransparentUpgradeableProxy transferProxy = new TransparentUpgradeableProxy(
            address(ics20TransferLogic),
            address(this),
            abi.encodeWithSelector(ICS20Transfer.initialize.selector, address(routerProxy))
        );

        transferProxyAdmin = ProxyAdmin(_getAdminFromLogs(vm.getRecordedLogs(), address(transferProxy)));

        // ============== Step 3: Wire up the contracts ==============
        ics26Router = ICS26Router(address(routerProxy));
        ics20Transfer = ICS20Transfer(address(transferProxy));
        erc20 = new TestERC20();

        clientIdentifier = ics26Router.ICS04_CHANNEL().addChannel(
            "07-tendermint", IICS04ChannelMsgs.Channel(counterpartyId, merklePrefix), address(lightClient)
        );

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp(ICS20Lib.DEFAULT_PORT_ID)));
    }

    function test_success_upgrade() public {
        // ============== Step 4: Migrate the contracts ==============
        DummyInitializable newLogic = new DummyInitializable();

        transferProxyAdmin.upgradeAndCall(
            ITransparentUpgradeableProxy(address(ics20Transfer)),
            address(newLogic),
            abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );
    }

    function test_failure_upgrade() public {
        // ============== Step 4: Migrate the contracts ==============
        ErroneousInitializable newLogic = new ErroneousInitializable();

        vm.expectRevert(abi.encodeWithSelector(ErroneousInitializable.InitializeFailed.selector));
        transferProxyAdmin.upgradeAndCall(
            ITransparentUpgradeableProxy(address(ics20Transfer)),
            address(newLogic),
            abi.encodeWithSelector(DummyInitializable.initializeV2.selector)
        );
    }

    function _getAdminFromLogs(VmSafe.Log[] memory logs, address emitter) internal pure returns (address) {
        for (uint256 i = 0; i < logs.length; i++) {
            if (logs[i].emitter == emitter && logs[i].topics[0] == IERC1967.AdminChanged.selector) {
                (, address newAdmin) = abi.decode(logs[i].data, (address, address));
                return newAdmin;
            }
        }
        revert("Admin not found");
    }
}
