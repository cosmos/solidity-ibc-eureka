// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Test } from "forge-std/Test.sol";
import { TransparentUpgradeableProxy, ITransparentUpgradeableProxy } from "@openzeppelin/proxy/transparent/TransparentUpgradeableProxy.sol";
import { Strings } from "@openzeppelin/utils/Strings.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICSCore } from "../../contracts/ICSCore.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS04ChannelMsgs } from "../../contracts/msgs/IICS04ChannelMsgs.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { TestERC20 } from "./mocks/TestERC20.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";

contract MigrationTest is Test {
    /// @notice The admin of the transparent proxies
    address admin;

    /// @notice The transperant proxy for the ICS26Router
    TransparentUpgradeableProxy ics26RouterProxy;
    /// @notice The transperant proxy for the ICS20Transfer
    TransparentUpgradeableProxy ics20TransferProxy;
    /// @notice The transperant proxy for the ICSCore
    TransparentUpgradeableProxy icsCoreProxy;


    /// @notice The logic contract for the ICS26Router
    ICS26Router public ics26Router;
    /// @notice The logic contract for the ICS20Transfer
    ICS20Transfer public ics20Transfer;
    /// @notice The logic contract for the ICSCore
    ICSCore public icsCore;

    TestERC20 public erc20;
    DummyLightClient public lightClient;

    string public clientId;
    string public counterpartyId = "42-dummy-01";
    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function setUp() public {
        admin = makeAddr("admin");

        ics26Router = new ICS26Router(address(this));
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        ics20Transfer = new ICS20Transfer(address(ics26Router));
        erc20 = new TestERC20();

        ics26RouterProxy = new TransparentUpgradeableProxy(address(ics26Router), admin, "");
        ics20TransferProxy = new TransparentUpgradeableProxy(address(ics20Transfer), admin, "");
        icsCoreProxy = new TransparentUpgradeableProxy(address(icsCore), admin, "");

        clientId = ics26Router.ICS04_CHANNEL().addChannel(
            "07-tendermint", IICS04ChannelMsgs.Channel(counterpartyId, merklePrefix), address(lightClient)
        );

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp(ICS20Lib.DEFAULT_PORT_ID)));
    }
}
