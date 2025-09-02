// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

/// @title CosmosICS27Lib
/// @notice This library provides utility functions for sending GMPs to Cosmos SDK chains.
library CosmosICS27Lib {
    /// @notice BRIDGE_RECEIVE_TYPE_URL is the type URL for the MsgBridgeReceive message in the TokenFactory module.
    string private constant BRIDGE_RECEIVE_TYPE_URL = "/wfchain.tokenfactory.MsgBridgeReceive";
    // solhint-disable-previous-line gas-small-strings

    /// @notice Wraps the provided message bytes in a JSON payload with a "messages" array.
    /// @param msg_ The message bytes to wrap.
    /// @return The JSON payload as bytes.
    function msgsToPayload(bytes memory msg_) internal pure returns (bytes memory) {
        return abi.encodePacked("{\"messages\":[", msg_, "]}");
    }

    /// @notice Constructs a MsgBridgeReceive message for the TokenFactory module.
    /// @param icaAddress The address of the cosmos ica.
    /// @param counterpartyClient The counterparty client id.
    /// @param receiver The address on the cosmos chain to receive the tokens.
    /// @param denom The denomination of the token to mint.
    /// @param amount The amount of tokens to mint.
    /// @return The encoded MsgMint message as bytes.
    function tokenFactoryBridgeReceiveMsg(
        string memory icaAddress,
        string memory counterpartyClient,
        string memory receiver,
        string memory denom,
        uint256 amount
    )
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(
            "{\"@type\":\"",
            BRIDGE_RECEIVE_TYPE_URL,
            "\",\"ica_address\":\"",
            icaAddress,
            "\",\"client_id\":\"",
            counterpartyClient,
            "\",\"receiver\":\"",
            receiver,
            "\",\"amount\":{\"denom\":\"",
            denom,
            "\",\"amount\":\"",
            Strings.toString(amount),
            "\"}}"
        );
    }
}
