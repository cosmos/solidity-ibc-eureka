// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title IFT Send Call Constructor Interface
/// @notice Interface for constructing ICS27-GMP call data for minting IFT tokens on counterparty chains
/// @dev Implementations may be stateless (pure functions) or stateful (e.g., storing denom for Cosmos SDK chains)
interface IIFTSendCallConstructor {
    /// @notice Constructs the ICS27-GMP call data for minting IFT tokens on the counterparty chain
    /// @dev The constructed call data should conform to the expected format of the counterparty IFT
    /// contract's mint function
    /// @param receiver The address of the receiver on the counterparty chain
    /// @param amount The amount of tokens to mint
    /// @return The constructed call data for the ICS27-GMP message
    function constructMintCall(string calldata receiver, uint256 amount) external view returns (bytes memory);
}
