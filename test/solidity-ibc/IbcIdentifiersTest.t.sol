// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable max-line-length,gas-custom-errors

import { Test } from "forge-std/Test.sol";
import { IBCIdentifiers } from "../../contracts/utils/IBCIdentifiers.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";

contract IBCIdentifiersTest is Test {
    struct ValidateIBCIdentifierTestCase {
        string m;
        string id;
        bool expPass;
    }

    function test_validateIBCIdentifier() public {
        // The following test cases are based on the test cases of ibc-go:
        // https://github.com/cosmos/ibc-go/blob/e443a88e0f2c84c131c5a1de47945a5733ff9c91/modules/core/24-host/validate_test.go#L57
        ValidateIBCIdentifierTestCase[] memory testCases = new ValidateIBCIdentifierTestCase[](15);
        testCases[0] = ValidateIBCIdentifierTestCase({ m: "valid lowercase", id: "transfer", expPass: true });
        testCases[1] =
            ValidateIBCIdentifierTestCase({ m: "valid id special chars", id: "._+-#[]<>._+-#[]<>", expPass: true });
        testCases[2] = ValidateIBCIdentifierTestCase({
            m: "valid id lower and special chars",
            id: "lower._+-#[]<>",
            expPass: true
        });
        testCases[3] = ValidateIBCIdentifierTestCase({ m: "numeric id", id: "1234567890", expPass: true });
        testCases[4] = ValidateIBCIdentifierTestCase({ m: "uppercase id", id: "NOTLOWERCASE", expPass: true });
        testCases[5] = ValidateIBCIdentifierTestCase({ m: "numeric id", id: "1234567890", expPass: true });
        testCases[6] = ValidateIBCIdentifierTestCase({ m: "blank id", id: "               ", expPass: false });
        testCases[7] = ValidateIBCIdentifierTestCase({ m: "id length out of range", id: "1", expPass: false });
        testCases[8] = ValidateIBCIdentifierTestCase({
            m: "id is too long",
            id: "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Duis eros neque, ultricies vel ligula ac, convallis porttitor elit. Maecenas tincidunt turpis elit, vel faucibus nisl pellentesque sodales",
            expPass: false
        });
        testCases[9] = ValidateIBCIdentifierTestCase({ m: "path-like id", id: "lower/case/id", expPass: false });
        testCases[10] = ValidateIBCIdentifierTestCase({ m: "invalid id", id: "(clientid)", expPass: false });
        testCases[11] = ValidateIBCIdentifierTestCase({ m: "empty string", id: "", expPass: false });
        testCases[12] = ValidateIBCIdentifierTestCase({ m: "client prefix id", id: "client-5", expPass: false });
        testCases[13] = ValidateIBCIdentifierTestCase({ m: "channel prefix id", id: "channel-0", expPass: false });
        testCases[14] = ValidateIBCIdentifierTestCase({ m: "contract address", id: Strings.toHexString(makeAddr("test")), expPass: true });

        for (uint256 i = 0; i < testCases.length; i++) {
            ValidateIBCIdentifierTestCase memory tc = testCases[i];
            bool res = IBCIdentifiers.validateIBCIdentifier(bytes(tc.id));
            if (tc.expPass) {
                require(res, tc.m);
            } else {
                require(!res, tc.m);
            }
        }
    }
}
