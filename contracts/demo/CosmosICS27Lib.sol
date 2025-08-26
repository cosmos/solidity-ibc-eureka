// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

/// @title CosmosICS27Lib
/// @notice This library provides utility functions for sending GMPs to Cosmos SDK chains.
library CosmosICS27Lib {
    function getTokenFactoryMintMsg(
        string memory from,
        string memory receiver,
        uint256 amount
    )
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(
            "{\"messages\":[{\"@type\":\"/circle.fiattokenfactory.v1.MsgMint\",\"from\":\"",
            from,
            "\",\"address\":\"",
            receiver,
            "\",\"amount\":{\"denom\":\"factory/TODO/denom\",\"amount\":\"",
            Strings.toString(amount),
            "\"}}]}"
        );
    }
}
