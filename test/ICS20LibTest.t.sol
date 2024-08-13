// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICS20Lib } from "../src/utils/ICS20Lib.sol";

contract ICS20LibTest is Test {
    string[] public fixtureValidChannelID = [
        "channel-0",
        "channel-1",
        "channel-42",
        "channel-1000000",
        "07-tendermint-0",
        "07-tendermint-1",
        "07-tendermint-42",
        "07-tendermint-1000000",
        "42-dummy-0",
        "42-dummy-1",
        "42-dummy-42",
        "42-dummy-1000000"
    ];
    bytes[] public fixtureInvalidChannelID = [
        bytes("channel-"),
        bytes("hannel-1"),
        bytes("channel_1"),
        bytes("channel1"),
        bytes("channel-1a"),
        bytes("07-tendermint-"),
        bytes("07-tendermint-1a"),
        bytes("07-tendermint_1"),
        bytes("07-tendermint1"),
        bytes("07_tendermint-0"),
        bytes("07tendermint-0"),
        bytes("07-tendermint0")
    ];

    function test_isNumber() pure public {
        bytes memory numberData = "1234567890";

        for (uint256 i = 0; i < numberData.length; i++) {
            assertTrue(ICS20Lib.isNumber(numberData[i]));
        }

        bytes memory nonNumberData = "abcdefghijklmnopqrstuvwxyz!\"#$%&/()=-_";
        for (uint256 i = 0; i < nonNumberData.length; i++) {
            assertFalse(ICS20Lib.isNumber(nonNumberData[i]));
        }
    }

    function test_isChar() pure public {
        bytes memory charData = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

        for (uint256 i = 0; i < charData.length; i++) {
            assertTrue(ICS20Lib.isChar(charData[i]));
        }

        bytes memory nonCharData = "1234567890!\"#$%&/()=-_";
        for (uint256 i = 0; i < nonCharData.length; i++) {
            assertFalse(ICS20Lib.isChar(nonCharData[i]));
        }
    }

    function testFuzz_isValidChannelIDWithValidChannelIDs() view public {
        for (uint256 i = 0; i < fixtureValidChannelID.length; i++) {
            assertTrue(ICS20Lib.isValidChannelID(bytes(fixtureValidChannelID[i])), fixtureValidChannelID[i]);
        }
    }

    function testFuzz_isValidChannelIDWithInvalidChannelIDs(bytes memory invalidChannelID) pure public {
        assertFalse(ICS20Lib.isValidChannelID(invalidChannelID), string(invalidChannelID));
    }

    function test_splitPath() pure public {
        bytes memory path = "transfer/channel-0/uatom";
        bytes[] memory split = ICS20Lib.splitPath(path);
        assertEq(split.length, 3);
        assertEq(split[0], "transfer");
        assertEq(split[1], "channel-0");
        assertEq(split[2], "uatom");

        path = "uatom";
        split = ICS20Lib.splitPath(path);
        assertEq(split.length, 1);
        assertEq(split[0], "uatom");

        path = "transfer/07-tendermint-0/transfer/channel-0/test/tt/uatom";
        split = ICS20Lib.splitPath(path);
        assertEq(split.length, 7);
        assertEq(split[0], "transfer");
        assertEq(split[1], "07-tendermint-0");
        assertEq(split[2], "transfer");
        assertEq(split[3], "channel-0");
        assertEq(split[4], "test");
        assertEq(split[5], "tt");
        assertEq(split[6], "uatom");
    }

    function test_extractDenomFromPath() pure public {
        bytes memory path = "transfer/channel-0/uatom";
        ICS20Lib.Denom memory denom = ICS20Lib.extractDenomFromPath(path);
        assertEq(denom.base, "uatom");
        assertEq(denom.trace.length, 1);
        assertEq(denom.trace[0].port, "transfer");
        assertEq(denom.trace[0].channel, "channel-0");

        path = "uatom";
        denom = ICS20Lib.extractDenomFromPath(path);
        assertEq(denom.base, "uatom");
        assertEq(denom.trace.length, 0);

        path = "transfer/07-tendermint-0/transfer/channel-0/test/tt/uatom";
        denom = ICS20Lib.extractDenomFromPath(path);
        assertEq(denom.base, "test/tt/uatom");
        assertEq(denom.trace.length, 2);
        assertEq(denom.trace[0].port, "transfer");
        assertEq(denom.trace[0].channel, "07-tendermint-0");
        assertEq(denom.trace[1].port, "transfer");
        assertEq(denom.trace[1].channel, "channel-0");
    }
}