// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

contract ICS24HostTest is Test {
    bytes[] public ibcPrefix = [bytes("ibc"), bytes("")];
    bytes[] public otherPrefix = [bytes("ibc"), bytes("test/")];

    struct IBCPrefixedPathTestCase {
        bytes[] prefix;
        bytes path;
        bytes[2] expected;
    }

    function test_prefixedPath() public view {
        // Test cases against the ibc-go implementations output
        IBCPrefixedPathTestCase[2] memory testCases = [
            IBCPrefixedPathTestCase(
                ibcPrefix,
                abi.encodePacked("clients/07-tendermint-0/clientState"),
                [bytes("ibc"), bytes("clients/07-tendermint-0/clientState")]
            ),
            IBCPrefixedPathTestCase(
                otherPrefix,
                abi.encodePacked("clients/07-tendermint-0/clientState"),
                [bytes("ibc"), bytes("test/clients/07-tendermint-0/clientState")]
            )
        ];

        for (uint256 i = 0; i < testCases.length; i++) {
            IBCPrefixedPathTestCase memory testCase = testCases[i];
            bytes[] memory actual = ICS24Host.prefixedPath(testCase.prefix, testCase.path);
            assertEq(actual.length, 2);
            assertEq(actual[0], testCase.expected[0]);
            assertEq(actual[1], testCase.expected[1]);
        }
    }

    function test_packetCommitment() public pure {
        // Test against the ibc-go implementations output
        IICS20TransferMsgs.Hop[] memory trace = new IICS20TransferMsgs.Hop[](1);
        trace[0] = IICS20TransferMsgs.Hop({ portId: "traceport", clientId: "client-0" });
        ICS20Lib.Token[] memory tokens = new ICS20Lib.Token[](1);
        tokens[0] = ICS20Lib.Token({ denom: ICS20Lib.Denom({ base: "uatom", trace: trace }), amount: 1_000_000 });

        IICS20TransferMsgs.Hop[] memory hops = new IICS20TransferMsgs.Hop[](1);
        hops[0] = IICS20TransferMsgs.Hop({ portId: "hopport", clientId: "client-1" });
        ICS20Lib.FungibleTokenPacketDataV2 memory packetData = ICS20Lib.FungibleTokenPacketDataV2({
            tokens: tokens,
            sender: "sender",
            receiver: "receiver",
            memo: "memo",
            forwarding: ICS20Lib.ForwardingPacketData({ destinationMemo: "destination-memo", hops: hops })
        });

        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(packetData)
        });

        IICS26RouterMsgs.Packet memory packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceClient: "07-tendermint-0",
            destClient: "07-tendermint-1",
            timeoutTimestamp: 100,
            payloads: payloads
        });

        bytes32 commitmentBytes = ICS24Host.packetCommitmentBytes32(packet);
        string memory actual = Strings.toHexString(uint256(commitmentBytes));
        string memory expected = "0x634d50b132aadb0395ceb840bb613191326b5fc47248fd50e9e5c622ca11b59f";
        assertEq(actual, expected);
    }

    function test_packetAcknowledgementCommitment() public pure {
        // Test against the ibc-go implementations output
        bytes memory ack = abi.encodePacked("some bytes");
        bytes[] memory acks = new bytes[](1);
        acks[0] = ack;
        bytes32 ackHash = ICS24Host.packetAcknowledgementCommitmentBytes32(acks);
        string memory actualAckHash = Strings.toHexString(uint256(ackHash));
        string memory expectedAckHash = "0xf03b4667413e56aaf086663267913e525c442b56fa1af4fa3f3dab9f37044c5b";
        assertEq(actualAckHash, expectedAckHash);
    }

    function test_packetKeys() public pure {
        // Test against the ibc-go implementations output
        bytes memory packetCommitmentKey = ICS24Host.packetCommitmentPathCalldata("channel-0", 1);
        string memory actualCommitmentKey = bytesToHex(packetCommitmentKey);
        string memory expectedCommitmentKey = "6368616e6e656c2d30010000000000000001";
        assertEq(actualCommitmentKey, expectedCommitmentKey);

        bytes memory packetReceiptCommitmentKey = ICS24Host.packetReceiptCommitmentPathCalldata("channel-1", 2);
        string memory actualReceiptCommitmentKey = bytesToHex(packetReceiptCommitmentKey);
        string memory expectedReceiptCommitmentKey = "6368616e6e656c2d31020000000000000002";
        assertEq(actualReceiptCommitmentKey, expectedReceiptCommitmentKey);

        bytes memory packetAcknowledgementCommitmentKey =
            ICS24Host.packetAcknowledgementCommitmentPathCalldata("channel-2", 3);
        string memory actualAcknowledgementCommitmentKey = bytesToHex(packetAcknowledgementCommitmentKey);
        string memory expectedAcknowledgementCommitmentKey = "6368616e6e656c2d32030000000000000003";
        assertEq(actualAcknowledgementCommitmentKey, expectedAcknowledgementCommitmentKey);
    }

    function bytesToHex(bytes memory data) public pure returns (string memory) {
        bytes memory alphabet = "0123456789abcdef";
        bytes memory str = new bytes(2 * data.length);
        for (uint256 i = 0; i < data.length; i++) {
            str[2 * i] = alphabet[uint8(data[i] >> 4)];
            str[2 * i + 1] = alphabet[uint8(data[i] & 0x0f)];
        }
        return string(str);
    }
}
