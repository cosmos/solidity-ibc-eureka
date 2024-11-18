// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICS20Lib } from "../src/utils/ICS20Lib.sol";
import { IICS20Errors } from "../src/errors/IICS20Errors.sol";

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

    function test_unmarshalJSON() public {
        // ICS20Lib marshalled json with memo
        bytes memory jsonBz = ICS20Lib.marshalJSON("denom", 42, "sender", "receiver", "memo");
        ICS20Lib.PacketDataJSON memory packetData = this.unmarshalJSON(jsonBz);
        assertEq(packetData.denom, "denom");
        assertEq(packetData.amount, 42);
        assertEq(packetData.sender, "sender");
        assertEq(packetData.receiver, "receiver");
        assertEq(packetData.memo, "memo");

        // ICS20Lib marshalled json without memo
        jsonBz = ICS20Lib.marshalJSON("denom2", 43, "sender2", "receiver2", "");
        packetData = this.unmarshalJSON(jsonBz);
        assertEq(packetData.denom, "denom2");
        assertEq(packetData.amount, 43);
        assertEq(packetData.sender, "sender2");
        assertEq(packetData.receiver, "receiver2");
        assertEq(packetData.memo, "");

        // Test with a manual JSON string with memo
        jsonBz =
            "{\"denom\":\"denom3\",\"amount\":\"43\",\"sender\":\"sender3\",\"receiver\":\"receiver3\",\"memo\":\"memo3\"}";
        packetData = this.unmarshalJSON(jsonBz);
        assertEq(packetData.denom, "denom3");
        assertEq(packetData.amount, 43);
        assertEq(packetData.sender, "sender3");
        assertEq(packetData.receiver, "receiver3");
        assertEq(packetData.memo, "memo3");

        // Test with a manual JSON string without memo
        jsonBz = "{\"denom\":\"denom3\",\"amount\":\"43\",\"sender\":\"sender3\",\"receiver\":\"receiver3\"}";
        packetData = this.unmarshalJSON(jsonBz);
        assertEq(packetData.denom, "denom3");
        assertEq(packetData.amount, 43);
        assertEq(packetData.sender, "sender3");
        assertEq(packetData.receiver, "receiver3");
        assertEq(packetData.memo, "");

        // Test with a broken JSON string without memo
        jsonBz = "{\"denom\":\"denom3\",\"amount\":\"43\",\"sender\":\"sender3\\,\"receiver\":\"receiver3\"}";
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20JSONInvalidEscape.selector, 50, bytes1(0x2c)));
        packetData = this.unmarshalJSON(jsonBz);
    }

    function unmarshalJSON(bytes calldata bz) external pure returns (ICS20Lib.PacketDataJSON memory) {
        return ICS20Lib.unmarshalJSON(bz);
    }
}
