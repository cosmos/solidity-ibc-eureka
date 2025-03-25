// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IIBCERC20 {
    /// @notice The role identifier for the metadata setter role
    /// @return The role identifier
    function METADATA_SETTER_ROLE() external view returns (bytes32);

    /// @notice Set the metadata for the token
    /// @dev This function can only be called by the metadata setter role
    /// @param decimals The decimals for the custom token metadata
    /// @param name The name for the custom token metadata
    /// @param symbol The symbol for the custom token metadata
    function setMetadata(uint8 decimals, string calldata name, string calldata symbol) external;

    /// @notice Mint new tokens to the Escrow contract
    /// @dev This function can only be called by the ICS20 contract
    /// @dev This function can only mint tokens to the Escrow contract
    /// @param mintAddress Address to mint tokens to
    /// @param amount Amount of tokens to mint
    function mint(address mintAddress, uint256 amount) external;

    /// @notice Burn tokens from the Escrow contract
    /// @dev This function can only be called by the ICS20 contract
    /// @dev This function can only burn tokens from the Escrow contract
    /// @param mintAddress Address to burn tokens from
    /// @param amount Amount of tokens to burn
    function burn(address mintAddress, uint256 amount) external;

    /// @notice Get the full denom path of the token
    /// @return The full path of the token's denom
    function fullDenomPath() external view returns (string memory);

    /// @notice Get the escrow contract address
    /// @return The escrow contract address
    function escrow() external view returns (address);

    /// @notice Get the ICS20 contract address
    /// @return The ICS20 contract address
    function ics20() external view returns (address);

    /// @notice Grant the metadata setter role to an account
    /// @dev This function can only be called by the token operator from ICS20
    /// @param account The account to grant the metadata setter role to
    function grantMetadataSetterRole(address account) external;

    /// @notice Revoke the metadata setter role from an account
    /// @dev This function can only be called by the token operator from ICS20
    /// @param account The account to revoke the metadata setter role from
    function revokeMetadataSetterRole(address account) external;

    /// @notice Initializes the IBCERC20 contract
    /// @dev This function is meant to be called by a proxy
    /// @param ics20_ The ICS20 contract address
    /// @param escrow_ The escrow contract address, can burn and mint tokens
    /// @param fullDenomPath_ The full IBC denom path for this token
    function initialize(address ics20_, address escrow_, string memory fullDenomPath_) external;
}
