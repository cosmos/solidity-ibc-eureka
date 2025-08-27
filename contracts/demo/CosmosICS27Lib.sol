// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

/// @title CosmosICS27Lib
/// @notice This library provides utility functions for sending GMPs to Cosmos SDK chains.
library CosmosICS27Lib {
    /// @notice MINT_TYPE_URL is the type URL for the MsgMint message in the FiatTokenFactory module.
    string private constant MINT_TYPE_URL = "/circle.fiattokenfactory.v1.MsgMint";
    // solhint-disable-previous-line gas-small-strings

    /// @notice Constructs a MsgMint message for the FiatTokenFactory module.
    /// @param from The address of the minter.
    /// @param receiver The address of the recipient.
    /// @param subdenom The subdenomination of the token to mint.
    /// @param amount The amount of tokens to mint.
    /// @return The encoded MsgMint message as bytes.
    function tokenFactoryMintMsg(
        string memory from,
        string memory receiver,
        string memory subdenom,
        uint256 amount
    )
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(
            "{\"messages\":[{\"@type\":\"",
            MINT_TYPE_URL,
            "\",\"from\":\"",
            from,
            "\",\"address\":\"",
            receiver,
            "\",\"amount\":{\"denom\":\"",
            tokenFactoryDenom(from, subdenom),
            "\",\"amount\":\"",
            Strings.toString(amount),
            "\"}}]}"
        );
    }

    /// @notice Constructs the denom string for a token in the FiatTokenFactory module.
    /// @param from The address of the minter.
    /// @param subdenom The subdenomination of the token.
    /// @return The constructed token factory denom string as bytes.
    function tokenFactoryDenom(string memory from, string memory subdenom) private pure returns (bytes memory) {
        return abi.encodePacked("factory/", from, "/", subdenom);
    }
}
