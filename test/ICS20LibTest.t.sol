// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICS20Lib } from "../src/utils/ICS20Lib.sol";

contract ICS20LibTest is Test {
    struct IBCDenomTestCase {
        string denom;
        string expected;
    }

    function test_toIBCDenom() public pure {
        // Test cases against the ibc-go implementations output
        IBCDenomTestCase[3] memory testCases = [
            IBCDenomTestCase(
                "transfer/channel-0/uatom", "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2"
            ),
            IBCDenomTestCase(
                "transfer/channel-0/transfer/channel-52/uatom",
                "ibc/869A01B76A1E87154A4253B3493A2CDD106F4AE6E8F4800C252006C13B20C226"
            ),
            IBCDenomTestCase(
                "transfer/07-tendermint-0/stake", "ibc/D33713CAB4FB7F6E46CDB160183F558E99AFDA3C3A0F22B358273D374BECAA18"
            )
        ];

        for (uint256 i = 0; i < testCases.length; i++) {
            IBCDenomTestCase memory testCase = testCases[i];
            string memory actual = ICS20Lib.toIBCDenom(testCase.denom);
            assertEq(actual, testCase.expected);
        }
    }

}
