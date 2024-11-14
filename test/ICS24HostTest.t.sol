// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICS24Host } from "../src/utils/ICS24Host.sol";
import { IICS26RouterMsgs } from "../src/msgs/IICS26RouterMsgs.sol";
import { ICS20Lib } from "../src/utils/ICS20Lib.sol";
import { Strings } from "@openzeppelin/utils/Strings.sol";

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
        bytes memory transferPayload = ICS20Lib.marshalJSON("uatom", 1_000_000, "sender", "receiver", "memo");
        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: "transfer",
            destPort: "transfer",
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: transferPayload
        });
        IICS26RouterMsgs.Packet memory packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: "channel-0",
            destChannel: "channel-1",
            timeoutTimestamp: 100,
            payloads: payloads
        });

        bytes32 commitmentBytes = ICS24Host.packetCommitmentBytes32(packet);
        string memory actual = Strings.toHexString(uint256(commitmentBytes));
        string memory expected = "0xc75fb6745b83fe67fb01d11cc01de73f9203386cb20f5ae6102080ae07e28a24";

        assertEq(actual, expected);
    }
}
