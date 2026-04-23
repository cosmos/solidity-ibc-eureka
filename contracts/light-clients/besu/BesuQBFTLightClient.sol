// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { BesuLightClientBase } from "./BesuLightClientBase.sol";

contract BesuQBFTLightClient is BesuLightClientBase {
    constructor(
        address ibcRouter,
        uint64 initialTrustedHeight,
        uint64 initialTrustedTimestamp,
        bytes32 initialTrustedStorageRoot,
        address[] memory initialTrustedValidators,
        uint64 trustingPeriod,
        uint64 maxClockDrift,
        address roleManager
    )
        BesuLightClientBase(
            ibcRouter,
            initialTrustedHeight,
            initialTrustedTimestamp,
            initialTrustedStorageRoot,
            initialTrustedValidators,
            trustingPeriod,
            maxClockDrift,
            roleManager
        )
    { }

    function _commitSealDigest(ParsedHeader memory header) internal pure override returns (bytes32) {
        bytes[] memory extraItems = new bytes[](5);
        extraItems[0] = _rlpItemBytes(header.extraDataItems[0]);
        extraItems[1] = _rlpItemBytes(header.extraDataItems[1]);
        extraItems[2] = _rlpItemBytes(header.extraDataItems[2]);
        extraItems[3] = _rlpItemBytes(header.extraDataItems[3]);
        extraItems[4] = hex"c0";

        bytes memory signingExtraData = _encodeRlpList(extraItems);
        bytes[] memory headerItems = new bytes[](header.headerItems.length);
        for (uint256 i = 0; i < header.headerItems.length; ++i) {
            headerItems[i] = i == 12 ? _encodeRlpBytes(signingExtraData) : _rlpItemBytes(header.headerItems[i]);
        }
        return keccak256(_encodeRlpList(headerItems));
    }
}
