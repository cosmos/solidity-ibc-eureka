// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.25;

interface IICS20Errors {
    /// @param msgSender Address of the message sender
    /// @param packetSender Address of the packet sender
    error ICS20MsgSenderIsNotPacketSender(address msgSender, address packetSender);

    /// @param sender Address whose tokens are being transferred
    error ICS20InvalidSender(string sender);

    /// @param amount Amount of tokens being transferred
    error ICS20InvalidAmount(uint256 amount);

    /// @param tokenContract Address of the token contract
    error ICS20InvalidTokenContract(string tokenContract);

    /// @param version Version string
    error ICS20UnexpectedVersion(string version);

    /// @param expected Expected balance of the ERC20 token for ICS20Transfer
    /// @param actual Actual balance of the ERC20 token for ICS20Transfer
    error ICS20UnexpectedERC20Balance(uint256 expected, uint256 actual);

    // ICS20Lib Errors:

    /// @param position position in packet data bytes
    /// @param expected expected bytes
    /// @param actual actual bytes
    error ICS20JSONUnexpectedBytes(uint256 position, bytes32 expected, bytes32 actual);

    /// @param position position in packet data bytes
    /// @param actual actual value
    error ICS20JSONClosingBraceNotFound(uint256 position, bytes1 actual);

    /// @param position position in packet data bytes
    /// @param actual actual value
    error ICS20JSONStringClosingDoubleQuoteNotFound(uint256 position, bytes1 actual);

    /// @param bz json string value
    /// @param position position in packet data bytes
    error ICS20JSONStringUnclosed(bytes bz, uint256 position);

    /// @param position position in packet data bytes
    /// @param actual actual value
    error ICS20JSONInvalidEscape(uint256 position, bytes1 actual);

    /// @param length length of the slice
    error ICS20BytesSliceOverflow(uint256 length);

    /// @param length length of the bytes
    /// @param start start index
    /// @param end end index
    error ICS20BytesSliceOutOfBounds(uint256 length, uint256 start, uint256 end);
}
