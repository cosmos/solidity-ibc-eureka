// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICS24Host } from "../src/utils/ICS24Host.sol";

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
}
