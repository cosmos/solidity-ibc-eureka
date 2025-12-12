// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIFTSendCallConstructor } from "../interfaces/IIFTSendCallConstructor.sol";

import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

/// @title Cosmos SDK IFT Send Call Constructor
/// @notice Constructs ICS27-GMP call data for minting IFT tokens on Cosmos SDK-based counterparty chains
/// @dev This implementation encodes mint calls in protojson format for Cosmos SDK IFT module
contract CosmosIFTSendCallConstructor is IIFTSendCallConstructor {
    /// @notice The type URL for the MsgIFTMint message in the TokenFactory module
    // natlint-disable-next-line MissingInheritdoc
    string public bridgeReceiveTypeUrl;

    /// @notice The denomination of the counterparty token on the Cosmos SDK chain
    // natlint-disable-next-line MissingInheritdoc
    string public denom;

    /// @notice The interchain account address of the submitter account on the Cosmos SDK chain
    /// @dev Required to set the signer of the MsgIFTMint message (Cosmos SDK limitation)
    // natlint-disable-next-line MissingInheritdoc
    string public icaAddress;

    /// @notice Creates a new CosmosIFTSendCallConstructor
    /// @param bridgeReceiveTypeUrl_ The type URL for MsgIFTMint
    /// @param denom_ The denom of the token on the Cosmos SDK chain (e.g., "uatom", "ibc/...")
    /// @param icaAddress_ The interchain account address on the Cosmos SDK chain
    constructor(string memory bridgeReceiveTypeUrl_, string memory denom_, string memory icaAddress_) {
        bridgeReceiveTypeUrl = bridgeReceiveTypeUrl_;
        denom = denom_;
        icaAddress = icaAddress_;
    }

    /// @inheritdoc IIFTSendCallConstructor
    /// @dev Encodes the mint call as protojson for Cosmos SDK IFT module
    function constructMintCall(string calldata receiver, uint256 amount) external view returns (bytes memory) {
        return abi.encodePacked(
            "{\"@type\":\"",
            bridgeReceiveTypeUrl,
            "\",\"ica_address\":\"",
            icaAddress,
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
