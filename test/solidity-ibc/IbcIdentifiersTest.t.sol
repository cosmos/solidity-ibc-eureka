// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable max-line-length,gas-custom-errors

import { Test } from "forge-std/Test.sol";
import { IBCIdentifiers } from "../../contracts/utils/IBCIdentifiers.sol";

contract IBCIdentifiersTest is Test {
    struct ValidatePortIdentifierTestCase {
        string m;
        string id;
        bool expPass;
    }

    function testValidatePortIdentifier() public pure {
        // The following test cases are based on the test cases of ibc-go:
        // https://github.com/cosmos/ibc-go/blob/e443a88e0f2c84c131c5a1de47945a5733ff9c91/modules/core/24-host/validate_test.go#L57
        ValidatePortIdentifierTestCase[] memory testCases = new ValidatePortIdentifierTestCase[](12);
        testCases[0] = ValidatePortIdentifierTestCase({ m: "valid lowercase", id: "transfer", expPass: true });
        testCases[1] =
            ValidatePortIdentifierTestCase({ m: "valid id special chars", id: "._+-#[]<>._+-#[]<>", expPass: true });
        testCases[2] = ValidatePortIdentifierTestCase({
            m: "valid id lower and special chars",
            id: "lower._+-#[]<>",
            expPass: true
        });
        testCases[3] = ValidatePortIdentifierTestCase({ m: "numeric id", id: "1234567890", expPass: true });
        testCases[4] = ValidatePortIdentifierTestCase({ m: "uppercase id", id: "NOTLOWERCASE", expPass: true });
        testCases[5] = ValidatePortIdentifierTestCase({ m: "numeric id", id: "1234567890", expPass: true });
        testCases[6] = ValidatePortIdentifierTestCase({ m: "blank id", id: "               ", expPass: false });
        testCases[7] = ValidatePortIdentifierTestCase({ m: "id length out of range", id: "1", expPass: false });
        testCases[8] = ValidatePortIdentifierTestCase({
            m: "id is too long",
            id: "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Duis eros neque, ultricies vel ligula ac, convallis porttitor elit. Maecenas tincidunt turpis elit, vel faucibus nisl pellentesque sodales",
            expPass: false
        });
        testCases[9] = ValidatePortIdentifierTestCase({ m: "path-like id", id: "lower/case/id", expPass: false });
        testCases[10] = ValidatePortIdentifierTestCase({ m: "invalid id", id: "(clientid)", expPass: false });
        testCases[11] = ValidatePortIdentifierTestCase({ m: "empty string", id: "", expPass: false });

        for (uint256 i = 0; i < testCases.length; i++) {
            ValidatePortIdentifierTestCase memory tc = testCases[i];
            bool res = IBCIdentifiers.validatePortIdentifier(bytes(tc.id));
            if (tc.expPass) {
                require(res, tc.m);
            } else {
                require(!res, tc.m);
            }
        }
    }
}
