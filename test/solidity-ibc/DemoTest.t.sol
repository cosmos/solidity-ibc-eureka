// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,max-states-count

import { Test } from "forge-std/Test.sol";

import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS27GMPMsgs } from "../../contracts/msgs/IICS27GMPMsgs.sol";

import { IERC20 } from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import { ISignatureTransfer } from "@uniswap/permit2/src/interfaces/ISignatureTransfer.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { IICS26RouterErrors } from "../../contracts/errors/IICS26RouterErrors.sol";
import { ILightClient } from "../../contracts/interfaces/ILightClient.sol";
import { IICS27Account } from "../../contracts/interfaces/IICS27Account.sol";

import { XERC20 } from "@defi-wonderland/xerc20/contracts/XERC20.sol";
import { IbcImpl } from "./utils/IbcImpl.sol";
import { TestHelper } from "./utils/TestHelper.sol";
import { IntegrationEnv } from "./utils/IntegrationEnv.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { ICS27Lib } from "../../contracts/utils/ICS27Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { RefImplIBCERC20 } from "./utils/RefImplIBCERC20.sol";

contract Integration2Test is Test {
    IbcImpl public ibcImplA;
    IbcImpl public ibcImplB;

    address public userA = makeAddr("userA");
    address public userB = makeAddr("userB");
    address public factory = makeAddr("factory");
    XERC20 public xerc20;

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

        xerc20 = new XERC20("Test", "WF", factory);

        // precompute account address
        IICS27GMPMsgs.AccountIdentifier memory accountId = IICS27GMPMsgs.AccountIdentifier({
            clientId: th.FIRST_CLIENT_ID(),
            sender: Strings.toHexString(userA),
            salt: ""
        });
        address computedAccount = ibcImplB.ics27Gmp().getOrComputeAccountAddress(accountId);

        vm.prank(factory);
        xerc20.setLimits(computedAccount, 100, 100);
    }

    function testXERC20() public {
        bytes memory payload = abi.encodeCall(XERC20.mint, (userB, 10));
        IICS26RouterMsgs.Packet memory sentPacket =
            ibcImplA.sendGmpAsUser(userA, Strings.toHexString(address(xerc20)), payload, "");
        bytes[] memory acks = ibcImplB.recvPacket(sentPacket);
        assertEq(acks.length, 1, "ack length mismatch");

        // check userB balance
        assertEq(xerc20.balanceOf(userB), 10, "userB xerc20 balance mismatch");
    }
}
