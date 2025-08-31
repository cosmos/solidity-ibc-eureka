// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,max-states-count

import { Test } from "forge-std/Test.sol";

import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS27GMPMsgs } from "../../contracts/msgs/IICS27GMPMsgs.sol";

import { IbcImpl } from "./utils/IbcImpl.sol";
import { TestHelper } from "./utils/TestHelper.sol";
import { IntegrationEnv } from "./utils/IntegrationEnv.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBCXERC20 } from "../../contracts/demo/IBCXERC20.sol";

contract DemoTest is Test {
    IbcImpl public ibcImplA;
    IbcImpl public ibcImplB;

    address public userA = makeAddr("userA");
    address public userB = makeAddr("userB");
    address public owner = makeAddr("owner");
    IBCXERC20 public xerc20;

    TestHelper public th = new TestHelper();
    IntegrationEnv public integrationEnv = new IntegrationEnv();

    function setUp() public {
        // Deploy the IBC implementation
        ibcImplA = new IbcImpl(integrationEnv.permit2());
        ibcImplB = new IbcImpl(integrationEnv.permit2());

        // Add the counterparty implementations
        string memory clientId;
        clientId = ibcImplA.addCounterpartyImpl(ibcImplB, th.FIRST_CLIENT_ID());
        assertEq(clientId, th.FIRST_CLIENT_ID());

        clientId = ibcImplB.addCounterpartyImpl(ibcImplA, th.FIRST_CLIENT_ID());
        assertEq(clientId, th.FIRST_CLIENT_ID());

        // precompute account address
        IICS27GMPMsgs.AccountIdentifier memory accountId = IICS27GMPMsgs.AccountIdentifier({
            clientId: th.FIRST_CLIENT_ID(),
            sender: Strings.toHexString(userA),
            salt: ""
        });
        address computedAccount = ibcImplB.ics27Gmp().getOrComputeAccountAddress(accountId);

        address xerc20Logic = address(new IBCXERC20());
        ERC1967Proxy xerc20Proxy = new ERC1967Proxy(
            xerc20Logic, abi.encodeCall(IBCXERC20.initialize, (owner, "WildFlower", "WF", address(ibcImplB.ics27Gmp())))
        );
        xerc20 = IBCXERC20(address(xerc20Proxy));
        vm.prank(owner);
        xerc20.setBridge(computedAccount);
    }

    function testXERC20() public {
        bytes memory payload = abi.encodeCall(IBCXERC20.mint, (userB, 10));
        IICS26RouterMsgs.Packet memory sentPacket =
            ibcImplA.sendGmpAsUser(userA, Strings.toHexString(address(xerc20)), payload, "");
        bytes[] memory acks = ibcImplB.recvPacket(sentPacket);
        assertEq(acks.length, 1, "ack length mismatch");

        // check userB balance
        assertEq(xerc20.balanceOf(userB), 10, "userB xerc20 balance mismatch");
    }
}
