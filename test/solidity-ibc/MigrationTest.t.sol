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
import { TransparentUpgradeableProxy } from "@openzeppelin/proxy/transparent/TransparentUpgradeableProxy.sol";

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
        ICSCore icsCoreLogic = new ICSCore();
        ICS26Router ics26RouterLogic = new ICS26Router();
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        // ============== Step 2: Deploy Transparent Proxies ==============
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
}
