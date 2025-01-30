// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";

contract ICS20LibTest is Test { 

    function test_getDenomIdentifier_short() pure public {
        ICS20Lib.Denom memory denom = ICS20Lib.Denom({ base: "uatom", trace: new ICS20Lib.Hop[](0) });
        
        ICS20Lib.getDenomIdentifier(denom);
    }

    function test_getIBCDenom_short() pure public {
        ICS20Lib.Denom memory denom = ICS20Lib.Denom({ base: "uatom", trace: new ICS20Lib.Hop[](0) });

        getIBCDenom(denom);
    }

    function test_getDenomIdentifier_long() pure public {
        ICS20Lib.Denom memory denom = ICS20Lib.Denom({ base: "uatom", trace: new ICS20Lib.Hop[](2) });
        denom.trace[0] = ICS20Lib.Hop({ portId: "transfer", clientId: "client-0" });
        denom.trace[1] = ICS20Lib.Hop({ portId: "transfer", clientId: "client-1" });
        
        ICS20Lib.getDenomIdentifier(denom);
    }

    function test_getIBCDenom_long() pure public {
        ICS20Lib.Denom memory denom = ICS20Lib.Denom({ base: "uatom", trace: new ICS20Lib.Hop[](2) });
        denom.trace[0] = ICS20Lib.Hop({ portId: "transfer", clientId: "client-0" });
        denom.trace[1] = ICS20Lib.Hop({ portId: "transfer", clientId: "client-1" });


        getIBCDenom(denom);
    }

    function getIBCDenom(ICS20Lib.Denom memory denom) public pure returns (string memory) {
        string memory fullDenomPath = getFullPath(denom);
        string memory hash = toHexHash(fullDenomPath);
        return string(abi.encodePacked("ibc/", hash));
    }

    function toHexHash(string memory str) public pure returns (string memory) {
        bytes32 hash = sha256(bytes(str));
        bytes memory hexBz = bytes(Strings.toHexString(uint256(hash)));

        // next we remove the `0x` prefix and uppercase the hash string
        bytes memory finalHex = new bytes(hexBz.length - 2); // we skip the 0x prefix

        for (uint256 i = 2; i < hexBz.length; i++) {
            // if lowercase a-z, convert to uppercase
            if (hexBz[i] >= 0x61 && hexBz[i] <= 0x7A) {
                finalHex[i - 2] = bytes1(uint8(hexBz[i]) - 32);
            } else {
                finalHex[i - 2] = hexBz[i];
            }
        }

        return string(finalHex);
    }

    function getFullPath(ICS20Lib.Denom memory denom) internal pure returns (string memory) {
        if (denom.trace.length == 0) {
            return denom.base;
        }

        string memory trace = "";
        for (uint256 i = 0; i < denom.trace.length; i++) {
            trace = string(abi.encodePacked(trace, denom.trace[i].portId, "/", denom.trace[i].clientId));
        }

        return string(abi.encodePacked(trace, "/", denom.base));
    }
}
