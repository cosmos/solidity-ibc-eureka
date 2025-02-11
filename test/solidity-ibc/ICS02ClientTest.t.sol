// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";

import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";

import { IICS02Client } from "../../contracts/interfaces/IICS02Client.sol";
import { ILightClient } from "../../contracts/interfaces/ILightClient.sol";
import { IAccessControl } from "@openzeppelin-contracts/access/IAccessControl.sol";

import { ICS02ClientUpgradeable } from "../../contracts/utils/ICS02ClientUpgradeable.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";

contract ICS02ClientTest is Test {
    ICS02ClientUpgradeable public ics02Client;
    DummyLightClient public lightClient;

    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];
    bytes[] public randomPrefix = [bytes("test"), bytes("prefix")];

    string public clientIdentifier;

    address public clientOwner = makeAddr("clientOwner");

    function setUp() public {
        ICS26Router ics26RouterLogic = new ICS26Router();
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);

        ERC1967Proxy routerProxy = new ERC1967Proxy(
            address(ics26RouterLogic), abi.encodeCall(ICS26Router.initialize, (address(this), address(this)))
        );
        ics02Client = ICS02ClientUpgradeable(address(routerProxy));

        vm.startPrank(clientOwner);
        string memory counterpartyId = "42-dummy-01";
        IICS02ClientMsgs.CounterpartyInfo memory counterpartyInfo =
            IICS02ClientMsgs.CounterpartyInfo(counterpartyId, merklePrefix);
        vm.expectEmit();
        emit IICS02Client.ICS02ClientAdded("client-0", counterpartyInfo);
        clientIdentifier = ics02Client.addClient(counterpartyInfo, address(lightClient));
        vm.stopPrank();

        ILightClient fetchedLightClient = ics02Client.getClient(clientIdentifier);
        assertNotEq(address(fetchedLightClient), address(0), "client not found");

        IICS02ClientMsgs.CounterpartyInfo memory fetchedCounterparty = ics02Client.getCounterparty(clientIdentifier);
        assertEq(fetchedCounterparty.clientId, counterpartyId, "counterparty not set correctly");

        bool hasRole = ics02Client.hasRole(ics02Client.getLightClientMigratorRole(clientIdentifier), clientOwner);
        assertTrue(hasRole, "client owner not set correctly");
    }

    function test_UpdateClient() public {
        bytes memory updateMsg = "testUpdateMsg";
        ILightClientMsgs.UpdateResult updateResult = ics02Client.updateClient(clientIdentifier, updateMsg);
        assertEq(uint256(updateResult), uint256(ILightClientMsgs.UpdateResult.Update), "updateClient failed");
        assertEq(updateMsg, lightClient.latestUpdateMsg(), "updateClient failed");
    }

    function test_MigrateClient() public {
        address bob = makeAddr("bob");

        vm.startPrank(bob);
        string memory counterpartyId = "42-dummy-01";
        DummyLightClient noopLightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.NoOp, 0, false);
        IICS02ClientMsgs.CounterpartyInfo memory counterpartyInfo =
            IICS02ClientMsgs.CounterpartyInfo(counterpartyId, randomPrefix);
        vm.expectEmit();
        emit IICS02Client.ICS02ClientAdded("client-1", counterpartyInfo);
        string memory substituteIdentifier = ics02Client.addClient(counterpartyInfo, address(noopLightClient));
        assertEq(2, ics02Client.getNextClientSeq());

        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector,
                bob,
                ics02Client.getLightClientMigratorRole(clientIdentifier)
            )
        );
        ics02Client.migrateClient(clientIdentifier, substituteIdentifier);
        vm.stopPrank();

        vm.startPrank(clientOwner);
        ics02Client.migrateClient(clientIdentifier, substituteIdentifier);
        ILightClient fetchedLightClient = ics02Client.getClient(clientIdentifier);
        assertEq(address(fetchedLightClient), address(noopLightClient), "client not migrated");
        vm.stopPrank();

        IICS02ClientMsgs.CounterpartyInfo memory fetchedCounterparty = ics02Client.getCounterparty(clientIdentifier);
        assertEq(fetchedCounterparty.clientId, counterpartyId, "counterparty not migrated");
        assertEq(fetchedCounterparty.merklePrefix, randomPrefix, "counterparty not migrated");
    }

    function test_RenounceRole() public {
        vm.startPrank(clientOwner);
        ics02Client.renounceRole(ics02Client.getLightClientMigratorRole(clientIdentifier), clientOwner);
        vm.stopPrank();

        bool hasRole = ics02Client.hasRole(ics02Client.getLightClientMigratorRole(clientIdentifier), clientOwner);
        assertFalse(hasRole, "client owner not renounced");
    }

    function test_Misbehaviour() public {
        bytes memory misbehaviourMsg = "testMisbehaviourMsg";
        ics02Client.submitMisbehaviour(clientIdentifier, misbehaviourMsg);
    }

    function test_UpgradeClient() public {
        bytes memory upgradeMsg = "testUpgradeMsg";
        ics02Client.upgradeClient(clientIdentifier, upgradeMsg);
    }
}
