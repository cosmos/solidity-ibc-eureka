// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

/// @title CosmosICS27Lib
/// @notice This library provides utility functions for sending GMPs to Cosmos SDK chains.
library CosmosICS27Lib {
    /// @notice MINT_TYPE_URL is the type URL for the MsgMint message in the TokenFactory module.
    string private constant MINT_TYPE_URL = "/wfchain.tokenfactory.MsgMint";
    // solhint-disable-previous-line gas-small-strings

    /// @notice CREATE_DENOM_TYPE_URL is the type URL for the MsgCreateDenom message in the TokenFactory module.
    string private constant CREATE_DENOM_TYPE_URL = "/wfchain.tokenfactory.MsgCreateDenom";
    // solhint-disable-previous-line gas-small-strings

    /// @notice Wraps the provided message bytes in a JSON payload with a "messages" array.
    /// @param msg_ The message bytes to wrap.
    /// @return The JSON payload as bytes.
    function msgsToPayload(bytes memory msg_) internal pure returns (bytes memory) {
        return abi.encodePacked("{\"messages\":[", msg_, "]}");
    }

    /// @notice Wraps two provided message bytes in a JSON payload with a "messages" array.
    /// @param msg1 The first message bytes to wrap.
    /// @param msg2 The second message bytes to wrap.
    /// @return The JSON payload as bytes.
    function msgsToPayload(bytes memory msg1, bytes memory msg2) internal pure returns (bytes memory) {
        return abi.encodePacked("{\"messages\":[", msg1, ",", msg2, "]}");
    }

    /// @notice Constructs a MsgMint message for the TokenFactory module.
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
            "{\"@type\":\"",
            MINT_TYPE_URL,
            "\",\"from\":\"",
            from,
            "\",\"address\":\"",
            receiver,
            "\",\"amount\":{\"denom\":\"",
            tokenFactoryDenom(from, subdenom),
            "\",\"amount\":\"",
            Strings.toString(amount),
            "\"}}"
        );
    }

    /// @notice Constructs a MsgCreateDenom message for the TokenFactory module.
    /// @param from The address of the minter.
    /// @param subdenom The subdenomination of the token to create.
    /// @return The encoded MsgCreateDenom message as bytes.
    function tokenFactoryCreateDenomMsg(
        string memory from,
        string memory subdenom
    )
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(
            "{\"@type\":\"", CREATE_DENOM_TYPE_URL, "\",\"sender\":\"", from, "\",\"subdenom\":\"", subdenom, "\"}"
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
