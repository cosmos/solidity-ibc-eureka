// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IIBCERC20 {
    /// @notice Mint new tokens to the Escrow contract
    /// @param amount Amount of tokens to mint
    function mint(uint256 amount) external;

    /// @notice Burn tokens from the Escrow contract
    /// @param amount Amount of tokens to burn
    function burn(uint256 amount) external;

    /// @notice Get the full denom path of the token
    /// @return the full path of the token's denom
    function fullDenomPath() external view returns (string memory);
}
