// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";

import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";

import { IICS02Client } from "../../contracts/interfaces/IICS02Client.sol";
import { ILightClient } from "../../contracts/interfaces/ILightClient.sol";
import { IAccessControl } from "@openzeppelin-contracts/access/IAccessControl.sol";
import { IICS02ClientErrors } from "../../contracts/errors/IICS02ClientErrors.sol";

import { ICS02ClientUpgradeable } from "../../contracts/utils/ICS02ClientUpgradeable.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { TestHelper } from "./utils/TestHelper.sol";

contract ICS02ClientTest is Test {
    ICS02ClientUpgradeable public ics02Client;
    address public lightClient = makeAddr("lightClient");

    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];
    bytes[] public randomPrefix = [bytes("test"), bytes("prefix")];

    string public clientIdentifier;

    address public clientOwner = makeAddr("clientOwner");
    TestHelper public th = new TestHelper();

    function setUp() public {
        ICS26Router ics26RouterLogic = new ICS26Router();

        ERC1967Proxy routerProxy =
            new ERC1967Proxy(address(ics26RouterLogic), abi.encodeCall(ICS26Router.initialize, (address(this))));
        ics02Client = ICS02ClientUpgradeable(address(routerProxy));

        ics02Client.grantRole(ics02Client.CLIENT_ID_CUSTOMIZER_ROLE(), address(this));
        ics02Client.grantRole(ics02Client.RELAYER_ROLE(), address(this));

        vm.startPrank(clientOwner);
        string memory counterpartyId = "42-dummy-01";
        IICS02ClientMsgs.CounterpartyInfo memory counterpartyInfo =
            IICS02ClientMsgs.CounterpartyInfo(counterpartyId, merklePrefix);
        vm.expectEmit();
        emit IICS02Client.ICS02ClientAdded(th.FIRST_CLIENT_ID(), counterpartyInfo, lightClient);
        clientIdentifier = ics02Client.addClient(counterpartyInfo, lightClient);
        vm.stopPrank();

        ILightClient fetchedLightClient = ics02Client.getClient(clientIdentifier);
        assertNotEq(address(fetchedLightClient), address(0), "client not found");

        IICS02ClientMsgs.CounterpartyInfo memory fetchedCounterparty = ics02Client.getCounterparty(clientIdentifier);
        assertEq(fetchedCounterparty.clientId, counterpartyId, "counterparty not set correctly");

        bool hasRole = ics02Client.hasRole(ics02Client.getLightClientMigratorRole(clientIdentifier), clientOwner);
        assertTrue(hasRole, "client owner not set correctly");
    }

    function test_success_customClientId() public {
        string memory customClientId = "custom-client-id";
        IICS02ClientMsgs.CounterpartyInfo memory counterpartyInfo =
            IICS02ClientMsgs.CounterpartyInfo(customClientId, merklePrefix);
        string memory newId = ics02Client.addClient(customClientId, counterpartyInfo, lightClient);
        assertEq(customClientId, newId, "custom client id not set correctly");
    }

    function test_failure_customClientId() public {
        // client id is not custom (starts with "client-")
        IICS02ClientMsgs.CounterpartyInfo memory counterpartyInfo =
            IICS02ClientMsgs.CounterpartyInfo(clientIdentifier, merklePrefix);
        vm.expectRevert(abi.encodeWithSelector(IICS02ClientErrors.IBCInvalidClientId.selector, clientIdentifier));
        ics02Client.addClient(clientIdentifier, counterpartyInfo, lightClient);

        // reuse of client id
        string memory customClientId = "custom-client-id";
        ics02Client.addClient(customClientId, counterpartyInfo, lightClient);
        vm.expectRevert(abi.encodeWithSelector(IICS02ClientErrors.IBCClientAlreadyExists.selector, customClientId));
        ics02Client.addClient(customClientId, counterpartyInfo, lightClient);
    }

    function test_MigrateClient() public {
        address bob = makeAddr("bob");

        vm.startPrank(bob);
        string memory counterpartyId = "42-dummy-01";
        address newLightClient = makeAddr("newLightClient");
        IICS02ClientMsgs.CounterpartyInfo memory counterpartyInfo =
            IICS02ClientMsgs.CounterpartyInfo(counterpartyId, randomPrefix);

        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector,
                bob,
                ics02Client.getLightClientMigratorRole(clientIdentifier)
            )
        );
        ics02Client.migrateClient(clientIdentifier, counterpartyInfo, newLightClient);
        vm.stopPrank();

        vm.startPrank(clientOwner);
        ics02Client.migrateClient(clientIdentifier, counterpartyInfo, newLightClient);
        ILightClient fetchedLightClient = ics02Client.getClient(clientIdentifier);
        assertEq(address(fetchedLightClient), newLightClient, "client not migrated");
        vm.stopPrank();

        IICS02ClientMsgs.CounterpartyInfo memory fetchedCounterparty = ics02Client.getCounterparty(clientIdentifier);
        assertEq(fetchedCounterparty.clientId, counterpartyId, "counterparty not migrated");
        assertEq(fetchedCounterparty.merklePrefix, randomPrefix, "counterparty not migrated");
        assertEq(ics02Client.getNextClientSeq(), 1, "client seq not incremented");
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
        bytes memory misbehaviourCall = abi.encodeCall(ILightClient.misbehaviour, (misbehaviourMsg));
        vm.mockCall(lightClient, misbehaviourCall, bytes(""));

        vm.expectCall(lightClient, misbehaviourCall);
        ics02Client.submitMisbehaviour(clientIdentifier, misbehaviourMsg);
    }

    function test_success_updateClient() public {
        bytes memory updateMsg = "testUpdateMsg";
        bytes memory updateCall = abi.encodeCall(ILightClient.updateClient, (updateMsg));
        vm.mockCall(lightClient, updateCall, abi.encode(ILightClientMsgs.UpdateResult(0)));

        vm.expectCall(lightClient, updateCall);
        ics02Client.updateClient(clientIdentifier, updateMsg);
    }

    function test_failure_updateClient() public {
        address unauthorized = makeAddr("unauthorized");
        bytes memory updateMsg = "testUpdateMsg";

        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, unauthorized, ics02Client.RELAYER_ROLE()
            )
        );
        vm.prank(unauthorized);
        ics02Client.updateClient(clientIdentifier, updateMsg);
    }
}
