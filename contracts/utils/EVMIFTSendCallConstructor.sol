// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIFTSendCallConstructor } from "../interfaces/IIFTSendCallConstructor.sol";
import { IIFT } from "../interfaces/IIFT.sol";

import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

/// @title EVM IFT Send Call Constructor
/// @notice Constructs ICS27-GMP call data for minting IFT tokens on EVM-based counterparty chains
contract EVMIFTSendCallConstructor is IIFTSendCallConstructor {
    /// @notice Error thrown when the receiver address is invalid
    /// @param receiver The invalid receiver string
    error EVMIFTInvalidReceiver(string receiver);

    /// @inheritdoc IIFTSendCallConstructor
    function constructMintCall(string calldata receiver, uint256 amount) external pure returns (bytes memory) {
        (bool success, address receiverAddr) = Strings.tryParseAddress(receiver);
        require(success, EVMIFTInvalidReceiver(receiver));

        return abi.encodeCall(IIFT.iftMint, (receiverAddr, amount));
    }
}
