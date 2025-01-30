// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";
import { TestERC20 } from "./mocks/TestERC20.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { DummyICS20Transfer } from "./mocks/DummyICS20Transfer.sol";
import { IICS20Transfer } from "../../contracts/interfaces/IICS20Transfer.sol";
import { Escrow } from "../../contracts/utils/Escrow.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS20Errors } from "../../contracts/errors/IICS20Errors.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import { IICS20Errors } from "../../contracts/errors/IICS20Errors.sol";

contract ICS20LibTest is Test, DummyICS20Transfer {
    function test_newMsgSendPacketV2() public {
        address sender = makeAddr("sender");

        TestERC20 erc20 = new TestERC20();
        ICS20Lib.Denom memory foreignDenom = ICS20Lib.Denom({ base: "uatom", trace: new IICS20TransferMsgs.Hop[](1) });
        foreignDenom.trace[0] = IICS20TransferMsgs.Hop({ portId: "transfer", clientId: "client-0" });
        IBCERC20 ibcERC20 = new IBCERC20(IICS20Transfer(address(this)), new Escrow(address(this)), foreignDenom);
        IICS20TransferMsgs.ERC20Token memory ibcERC20Token =
            IICS20TransferMsgs.ERC20Token({ contractAddress: address(ibcERC20), amount: 42_000 });

        IICS20TransferMsgs.ERC20Token memory erc20Token =
            IICS20TransferMsgs.ERC20Token({ contractAddress: address(erc20), amount: 100_069 });

        IICS20TransferMsgs.ERC20Token[] memory tokens = new IICS20TransferMsgs.ERC20Token[](1);
        tokens[0] = erc20Token;
        IICS20TransferMsgs.SendTransferMsg memory sendTransferMsg = IICS20TransferMsgs.SendTransferMsg({
            tokens: tokens,
            receiver: "receiver",
            sourceClient: "sourceclient-1",
            destPort: "transfer",
            timeoutTimestamp: 1337,
            memo: "memo",
            forwarding: IICS20TransferMsgs.Forwarding({ hops: new IICS20TransferMsgs.Hop[](0) })
        });

        // Test with normal ERC20 token
        IICS26RouterMsgs.MsgSendPacket memory packet = ICS20Lib.newMsgSendPacketV2(sender, sendTransferMsg);
        assertEq(packet.sourceClient, sendTransferMsg.sourceClient);
        assertEq(packet.timeoutTimestamp, sendTransferMsg.timeoutTimestamp);
        assertEq(packet.payloads.length, 1);
        ICS20Lib.FungibleTokenPacketDataV2 memory packetData =
            abi.decode(packet.payloads[0].value, (ICS20Lib.FungibleTokenPacketDataV2));
        assertEq(packetData.sender, Strings.toHexString(sender));
        assertEq(packetData.receiver, sendTransferMsg.receiver);
        assertEq(packetData.memo, sendTransferMsg.memo);
        assertEq(packetData.forwarding.destinationMemo, "");
        assertEq(packetData.forwarding.hops.length, 0);
        assertEq(packetData.tokens.length, 1);
        assertEq(packetData.tokens[0].amount, erc20Token.amount);
        assertEq(packetData.tokens[0].denom.base, Strings.toHexString(address(erc20)));
        assertEq(packetData.tokens[0].denom.trace.length, 0);

        // Test with IBCERC20 token
        tokens[0] = ibcERC20Token;
        sendTransferMsg.tokens = tokens;
        packet = ICS20Lib.newMsgSendPacketV2(sender, sendTransferMsg);
        assertEq(packet.sourceClient, sendTransferMsg.sourceClient);
        assertEq(packet.timeoutTimestamp, sendTransferMsg.timeoutTimestamp);
        assertEq(packet.payloads.length, 1);
        packetData = abi.decode(packet.payloads[0].value, (ICS20Lib.FungibleTokenPacketDataV2));
        assertEq(packetData.sender, Strings.toHexString(sender));
        assertEq(packetData.receiver, sendTransferMsg.receiver);
        assertEq(packetData.memo, sendTransferMsg.memo);
        assertEq(packetData.forwarding.destinationMemo, "");
        assertEq(packetData.forwarding.hops.length, 0);
        assertEq(packetData.tokens.length, 1);
        assertEq(packetData.tokens[0].amount, ibcERC20Token.amount);
        assertEq(packetData.tokens[0].denom.base, foreignDenom.base);
        assertEq(packetData.tokens[0].denom.trace.length, 1);
        assertEq(packetData.tokens[0].denom.trace[0].portId, foreignDenom.trace[0].portId);
        assertEq(packetData.tokens[0].denom.trace[0].clientId, foreignDenom.trace[0].clientId);
        // Reset tokens
        tokens[0] = erc20Token;
        sendTransferMsg.tokens = tokens;

        // Test with forwarding hops and memo
        IICS20TransferMsgs.Hop[] memory hops = new IICS20TransferMsgs.Hop[](1);
        hops[0] = IICS20TransferMsgs.Hop({ portId: "hopport", clientId: "client-1" });
        sendTransferMsg.forwarding = IICS20TransferMsgs.Forwarding({ hops: hops });
        sendTransferMsg.memo = "forwardingMemo";
        packet = ICS20Lib.newMsgSendPacketV2(sender, sendTransferMsg);
        assertEq(packet.sourceClient, sendTransferMsg.sourceClient);
        assertEq(packet.timeoutTimestamp, sendTransferMsg.timeoutTimestamp);
        assertEq(packet.payloads.length, 1);
        packetData = abi.decode(packet.payloads[0].value, (ICS20Lib.FungibleTokenPacketDataV2));
        assertEq(packetData.sender, Strings.toHexString(sender));
        assertEq(packetData.receiver, sendTransferMsg.receiver);
        assertEq(packetData.memo, "");
        assertEq(packetData.forwarding.destinationMemo, sendTransferMsg.memo);
        assertEq(packetData.forwarding.hops.length, 1);
        assertEq(packetData.forwarding.hops[0].portId, hops[0].portId);
        assertEq(packetData.forwarding.hops[0].clientId, hops[0].clientId);
        assertEq(packetData.tokens.length, 1);
        assertEq(packetData.tokens[0].amount, erc20Token.amount);
        assertEq(packetData.tokens[0].denom.base, Strings.toHexString(address(erc20)));
        assertEq(packetData.tokens[0].denom.trace.length, 0);
        // Reset forwarding and memo
        sendTransferMsg.forwarding = IICS20TransferMsgs.Forwarding({ hops: new IICS20TransferMsgs.Hop[](0) });
        sendTransferMsg.memo = "memo";

        // TODO: Test with multiple denoms #249

        // Test with empty tokens
        sendTransferMsg.tokens = new IICS20TransferMsgs.ERC20Token[](0);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAmount.selector, 0));
        ICS20Lib.newMsgSendPacketV2(sender, sendTransferMsg);
        // Reset tokens
        sendTransferMsg.tokens = tokens;

        // Test with invalid amount
        tokens[0] = IICS20TransferMsgs.ERC20Token({ contractAddress: address(erc20), amount: 0 });
        sendTransferMsg.tokens = tokens;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAmount.selector, 0));
        ICS20Lib.newMsgSendPacketV2(sender, sendTransferMsg);
        // Reset tokens
        tokens[0] = erc20Token;
        sendTransferMsg.tokens = tokens;
    }

    // Primarely here to make sure the identifier doesn't change - that would be bad...
    function test_getDenomIdentifier() public pure {
        // Contract address as base with no trace
        ICS20Lib.Denom memory denom = ICS20Lib.Denom({
            base: "0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496",
            trace: new IICS20TransferMsgs.Hop[](0)
        });

        bytes32 denomID = ICS20Lib.getDenomIdentifier(denom);
        assertEq(
            Strings.toHexString(uint256(denomID)), "0x4dad7666f675ed319e406ff19d399fbf60c24fe4e5ca034a387cedb7c131f7fb"
        );

        // Different contract address as base with no trace
        denom = ICS20Lib.Denom({
            base: "0x7FA9385bE102ac3EAc297483Dd6233D62b3e1497",
            trace: new IICS20TransferMsgs.Hop[](0)
        });
        denomID = ICS20Lib.getDenomIdentifier(denom);
        assertEq(
            Strings.toHexString(uint256(denomID)), "0xad394a13b52467c62521d0adbe8c823fae32ce6b3b8ed0469bf26b21a6cc6404"
        );

        // uatom as base with single hop trace
        denom = ICS20Lib.Denom({ base: "uatom", trace: new IICS20TransferMsgs.Hop[](1) });
        denom.trace[0] = IICS20TransferMsgs.Hop({ portId: "transfer", clientId: "07-tendermint-0" });
        denomID = ICS20Lib.getDenomIdentifier(denom);
        assertEq(
            Strings.toHexString(uint256(denomID)), "0x6b338325afbf52780db7a94eaa404da03f88bce4fd888c27ce316a5328204941"
        );

        // different base with single hop trace
        denom = ICS20Lib.Denom({ base: "differentbase", trace: new IICS20TransferMsgs.Hop[](1) });
        denom.trace[0] = IICS20TransferMsgs.Hop({ portId: "transfer", clientId: "07-tendermint-0" });
        denomID = ICS20Lib.getDenomIdentifier(denom);

        // Different portId with single hop trace
        denom = ICS20Lib.Denom({ base: "uatom", trace: new IICS20TransferMsgs.Hop[](1) });
        denom.trace[0] = IICS20TransferMsgs.Hop({ portId: "differentport", clientId: "07-tendermint-0" });
        denomID = ICS20Lib.getDenomIdentifier(denom);
        assertEq(
            Strings.toHexString(uint256(denomID)), "0xfce2cfb2362eec048e19c366e0729727e11775dd5a98e3fa895682ffd35a2c0c"
        );

        // Different clientId with single hop trace
        denom = ICS20Lib.Denom({ base: "uatom", trace: new IICS20TransferMsgs.Hop[](1) });
        denom.trace[0] = IICS20TransferMsgs.Hop({ portId: "transfer", clientId: "07-tendermint-1" });

        // Multiple hops
        denom = ICS20Lib.Denom({ base: "uatom", trace: new IICS20TransferMsgs.Hop[](2) });
        denom.trace[0] = IICS20TransferMsgs.Hop({ portId: "transfer", clientId: "07-tendermint-0" });
        denom.trace[1] = IICS20TransferMsgs.Hop({ portId: "transfer", clientId: "07-tendermint-1" });
        denomID = ICS20Lib.getDenomIdentifier(denom);
        assertEq(
            Strings.toHexString(uint256(denomID)), "0x1be8df7e437e6d6b10ba1e2441b52fcb535784b2d2eb146f97884dd35c1b67fa"
        );
    }

    function test_getPath() public pure {
        ICS20Lib.Denom memory denom = ICS20Lib.Denom({
            base: "0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496",
            trace: new IICS20TransferMsgs.Hop[](0)
        });
        string memory path = ICS20Lib.getPath(denom);
        assertEq(path, "0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496");

        denom = ICS20Lib.Denom({ base: "uatom", trace: new IICS20TransferMsgs.Hop[](1) });
        denom.trace[0] = IICS20TransferMsgs.Hop({ portId: "transfer", clientId: "07-tendermint-0" });
        path = ICS20Lib.getPath(denom);
        assertEq(path, "transfer/07-tendermint-0/uatom");

        denom = ICS20Lib.Denom({ base: "uatom", trace: new IICS20TransferMsgs.Hop[](2) });
        denom.trace[0] = IICS20TransferMsgs.Hop({ portId: "transfer", clientId: "07-tendermint-0" });
        denom.trace[1] = IICS20TransferMsgs.Hop({ portId: "transfer", clientId: "07-tendermint-1" });
        path = ICS20Lib.getPath(denom);
        assertEq(path, "transfer/07-tendermint-0/transfer/07-tendermint-1/uatom");

        denom = ICS20Lib.Denom({ base: "uatom", trace: new IICS20TransferMsgs.Hop[](0) });
        path = ICS20Lib.getPath(denom);
        assertEq(path, "uatom");
    }
}
