// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.25;

interface IICS20Errors {
    /// @notice Message sender is not the packet sender
    /// @param msgSender Address of the message sender
    /// @param packetSender Address of the packet sender
    error ICS20MsgSenderIsNotPacketSender(address msgSender, address packetSender);

    /// @notice Invalid sender address
    /// @param sender Address whose tokens are being transferred
    error ICS20InvalidSender(string sender);

    /// @notice Invalid receiver address
    /// @param receiver Address receiving the tokens
    error ICS20InvalidReceiver(string receiver);

    /// @notice Invalid transfer amount
    /// @param amount Amount of tokens being transferred
    error ICS20InvalidAmount(uint256 amount);

    /// @notice Invalid ERC20 token contract
    /// @param tokenContract Address of the token contract
    error ICS20InvalidTokenContract(string tokenContract);

    /// @notice Unexpected packet data version
    /// @param expected expected version of the packet data
    /// @param version actual version of the packet data
    error ICS20UnexpectedVersion(string expected, string version);

    /// @notice Unexpected ERC20 balance
    /// @param expected Expected balance of the ERC20 token for ICS20Transfer
    /// @param actual Actual balance of the ERC20 token for ICS20Transfer
    error ICS20UnexpectedERC20Balance(uint256 expected, uint256 actual);

    /// @notice Unsupported feature
    /// @param feature Unsupported feature
    error ICS20UnsupportedFeature(string feature);

    // ICS20Lib Errors:

    /// @notice Unexpected bytes in JSON packet data
    /// @param position position in packet data bytes
    /// @param expected expected bytes
    /// @param actual actual bytes
    error ICS20JSONUnexpectedBytes(uint256 position, bytes32 expected, bytes32 actual);

    /// @notice JSON closing brace not found
    /// @param position position in packet data bytes
    /// @param actual actual value
    error ICS20JSONClosingBraceNotFound(uint256 position, bytes1 actual);

    /// @notice JSON closing double quote not found
    /// @param position position in packet data bytes
    /// @param actual actual value
    error ICS20JSONStringClosingDoubleQuoteNotFound(uint256 position, bytes1 actual);

    /// @notice JSON string unclosed
    /// @param bz json string value
    /// @param position position in packet data bytes
    error ICS20JSONStringUnclosed(bytes bz, uint256 position);

    /// @notice JSON invalid escape
    /// @param position position in packet data bytes
    /// @param actual actual value
    error ICS20JSONInvalidEscape(uint256 position, bytes1 actual);

    /// @notice JSON bytes slice overflow
    /// @param length length of the slice
    error ICS20BytesSliceOverflow(uint256 length);

    /// @notice JSON bytes slice out of bounds
    /// @param length length of the bytes
    /// @param start start index
    /// @param end end index
    error ICS20BytesSliceOutOfBounds(uint256 length, uint256 start, uint256 end);
}
