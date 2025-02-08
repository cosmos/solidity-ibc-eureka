// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

interface IICS20Errors {
    /// @notice Unauthorized function call
    /// @param caller The caller of the function
    error ICS20Unauthorized(address caller);

    /// @notice Unauthorized packet sender
    /// @param packetSender Address of the message sender
    error ICS20UnauthorizedPacketSender(address packetSender);

    /// @notice Invalid address
    /// @param addr Address of the sender or receiver
    error ICS20InvalidAddress(string addr);

    /// @notice Invalid transfer amount
    /// @param amount Amount of tokens being transferred
    error ICS20InvalidAmount(uint256 amount);

    /// @notice Unexpected packet data version
    /// @param expected expected version of the packet data
    /// @param version actual version of the packet data
    error ICS20UnexpectedVersion(string expected, string version);

    /// @notice Unexpected ERC20 balance
    /// @param expected Expected balance of the ERC20 token for ICS20Transfer
    /// @param actual Actual balance of the ERC20 token for ICS20Transfer
    error ICS20UnexpectedERC20Balance(uint256 expected, uint256 actual);

    /// @notice this error happens when the denom has no foreign ibcERC20 contract (i.e. we don't know this denom)
    /// @param denom Denomination of the token being transferred, for which we have no foreign ibcERC20 contract
    error ICS20DenomNotFound(string denom);

    /// @notice Unsupported feature
    /// @param feature Unsupported feature
    error ICS20UnsupportedFeature(string feature);

    // ICS20Lib Errors:

    /// @notice Abi encoding/decoding failure
    error ICS20AbiEncodingFailure();
}
