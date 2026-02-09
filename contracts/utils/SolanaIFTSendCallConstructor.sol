// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIFTSendCallConstructor } from "../interfaces/IIFTSendCallConstructor.sol";

import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ERC165 } from "@openzeppelin-contracts/utils/introspection/ERC165.sol";
import { IERC165 } from "@openzeppelin-contracts/utils/introspection/IERC165.sol";

/// @title Solana IFT Send Call Constructor
/// @notice Constructs ICS27-GMP call data for minting IFT tokens on Solana counterparty chains
/// @dev Returns ABI-encoded (receiver, amount). Relayer builds GmpSolanaPayload with proper
///      PDA derivation, Borsh encoding, and protobuf wrapping.
contract SolanaIFTSendCallConstructor is IIFTSendCallConstructor, ERC165 {
    /// @dev Length of "0x" prefix (2) + 32-byte pubkey as hex (64) = 66 chars
    uint256 private constant SOLANA_PUBKEY_HEX_LENGTH = 66;

    error SolanaIFTInvalidReceiver(string receiver);

    /// @inheritdoc IIFTSendCallConstructor
    function constructMintCall(string calldata receiver, uint256 amount) external pure returns (bytes memory) {
        if (bytes(receiver).length != SOLANA_PUBKEY_HEX_LENGTH) revert SolanaIFTInvalidReceiver(receiver);
        (bool success, uint256 parsed) = Strings.tryParseHexUint(receiver);
        if (!success) revert SolanaIFTInvalidReceiver(receiver);

        return abi.encode(bytes32(parsed), amount);
    }

    /// @inheritdoc ERC165
    function supportsInterface(bytes4 interfaceId) public view virtual override(ERC165, IERC165) returns (bool) {
        return interfaceId == type(IIFTSendCallConstructor).interfaceId || super.supportsInterface(interfaceId);
    }
}
