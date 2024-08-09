// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.25;

// solhint-disable no-inline-assembly

import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import { IICS20Errors } from "../errors/IICS20Errors.sol";

// This library is mostly copied, with minor adjustments, from https://github.com/hyperledger-labs/yui-ibc-solidity
library ICS20Lib {
    /// @notice PacketDataJSON is the JSON representation of a fungible token transfer packet.
    /// @dev PacketData is defined in
    /// [ICS-20](https://github.com/cosmos/ibc/tree/main/spec/app/ics-020-fungible-token-transfer).
    /// @param denom The denomination of the token
    /// @param sender The sender of the token
    /// @param receiver The receiver of the token
    /// @param amount The amount of tokens
    /// @param memo Optional memo
    struct PacketDataJSON {
        string denom;
        string sender;
        string receiver;
        uint256 amount;
        string memo;
    }

    /// @notice Convenience type used after unmarshalling the packet data and converting addresses
    /// @param erc20ContractAddress The address of the ERC20 contract
    /// @param amount The amount of tokens
    /// @param sender The sender of the tokens
    /// @param receiver The receiver of the tokens
    /// @param memo Optional memo
    struct UnwrappedFungibleTokenPacketData {
        address erc20ContractAddress;
        uint256 amount;
        address sender;
        string receiver;
        string memo;
    }

    /// @notice ICS20_VERSION is the version string for ICS20 packet data.
    string public constant ICS20_VERSION = "ics20-1";

    /// @notice SUCCESSFUL_ACKNOWLEDGEMENT_JSON is the JSON bytes for a successful acknowledgement.
    bytes public constant SUCCESSFUL_ACKNOWLEDGEMENT_JSON = bytes("{\"result\":\"AQ==\"}");
    /// @notice FAILED_ACKNOWLEDGEMENT_JSON is the JSON bytes for a failed acknowledgement.
    bytes public constant FAILED_ACKNOWLEDGEMENT_JSON = bytes("{\"error\":\"failed\"}");
    /// @notice KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON is the keccak256 hash of SUCCESSFUL_ACKNOWLEDGEMENT_JSON.
    bytes32 internal constant KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON = keccak256(SUCCESSFUL_ACKNOWLEDGEMENT_JSON);

    /// @notice CHAR_DOUBLE_QUOTE is the ASCII value for double quote.
    uint256 private constant CHAR_DOUBLE_QUOTE = 0x22;
    /// @notice CHAR_SLASH is the ASCII value for slash.
    uint256 private constant CHAR_SLASH = 0x2f;
    /// @notice CHAR_BACKSLASH is the ASCII value for backslash.
    uint256 private constant CHAR_BACKSLASH = 0x5c;
    /// @notice CHAR_F is the ASCII value for 'f'.
    uint256 private constant CHAR_F = 0x66;
    /// @notice CHAR_R is the ASCII value for 'r'.
    uint256 private constant CHAR_R = 0x72;
    /// @notice CHAR_N is the ASCII value for 'n'.
    uint256 private constant CHAR_N = 0x6e;
    /// @notice CHAR_B is the ASCII value for 'b'.
    uint256 private constant CHAR_B = 0x62;
    /// @notice CHAR_T is the ASCII value for 't'.
    uint256 private constant CHAR_T = 0x74;
    /// @notice CHAR_CLOSING_BRACE is the ASCII value for closing brace '}'.
    uint256 private constant CHAR_CLOSING_BRACE = 0x7d;
    /// @notice CHAR_M is the ASCII value for 'm'.
    uint256 private constant CHAR_M = 0x6d;

    /// @notice HEX_DIGITS are the hex digits.
    bytes16 private constant HEX_DIGITS = "0123456789abcdef";

    /// @notice marshalUnsafeJSON marshals PacketData into JSON bytes without escaping.
    /// @dev `memo` field is omitted if it is empty. TODO: Consider if this should be changed.
    /// @param data PacketData to marshal
    /// @return Marshalled JSON bytes
    function marshalUnsafeJSON(PacketDataJSON memory data) internal pure returns (bytes memory) {
        if (bytes(data.memo).length == 0) {
            return marshalJSON(data.denom, data.amount, data.sender, data.receiver);
        } else {
            return marshalJSON(data.denom, data.amount, data.sender, data.receiver, data.memo);
        }
    }

    /// @notice marshalJSON marshals PacketData into JSON bytes with escaping.
    /// @param escapedDenom Escaped denom
    /// @param amount Amount
    /// @param escapedSender Escaped sender
    /// @param escapedReceiver Escaped receiver
    /// @param escapedMemo Escaped memo
    /// @return Marshalled JSON bytes
    function marshalJSON(
        string memory escapedDenom,
        uint256 amount,
        string memory escapedSender,
        string memory escapedReceiver,
        string memory escapedMemo
    )
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(
            "{\"denom\":\"",
            escapedDenom,
            "\",\"amount\":\"",
            Strings.toString(amount),
            "\",\"sender\":\"",
            escapedSender,
            "\",\"receiver\":\"",
            escapedReceiver,
            "\",\"memo\":\"",
            escapedMemo,
            "\"}"
        );
    }

    /// @notice marshalJSON marshals PacketData into JSON bytes with escaping.
    /// @param escapedDenom Escaped denom
    /// @param amount Amount
    /// @param escapedSender Escaped sender
    /// @param escapedReceiver Escaped receiver
    /// @return Marshalled JSON bytes
    function marshalJSON(
        string memory escapedDenom,
        uint256 amount,
        string memory escapedSender,
        string memory escapedReceiver
    )
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(
            "{\"denom\":\"",
            escapedDenom,
            "\",\"amount\":\"",
            Strings.toString(amount),
            "\",\"sender\":\"",
            escapedSender,
            "\",\"receiver\":\"",
            escapedReceiver,
            "\"}"
        );
    }

    /// @notice unmarshalJSON unmarshals JSON bytes into PacketData.
    /// @param bz JSON bytes
    /// @return Unmarshalled PacketData
    function unmarshalJSON(bytes calldata bz) internal pure returns (PacketDataJSON memory) {
        // TODO: Consider if this should support other orders of fields (currently fixed order: denom, amount, etc)
        PacketDataJSON memory pd;
        uint256 pos = 0;

        unchecked {
            if (bytes32(bz[pos:pos + 10]) != bytes32("{\"denom\":\"")) {
                revert IICS20Errors.ICS20JSONUnexpectedBytes(pos, bytes32("{\"denom\":\""), bytes32(bz[pos:pos + 10]));
            }
            (pd.denom, pos) = parseString(bz, pos + 10);

            if (bytes32(bz[pos:pos + 11]) != bytes32(",\"amount\":\"")) {
                revert IICS20Errors.ICS20JSONUnexpectedBytes(pos, bytes32("{\"amount\":\""), bytes32(bz[pos:pos + 11]));
            }
            (pd.amount, pos) = parseUint256String(bz, pos + 11);

            if (bytes32(bz[pos:pos + 11]) != bytes32(",\"sender\":\"")) {
                revert IICS20Errors.ICS20JSONUnexpectedBytes(pos, bytes32(",\"sender\":\""), bytes32(bz[pos:pos + 11]));
            }
            (pd.sender, pos) = parseString(bz, pos + 11);

            if (bytes32(bz[pos:pos + 13]) != bytes32(",\"receiver\":\"")) {
                revert IICS20Errors.ICS20JSONUnexpectedBytes(
                    pos, bytes32(",\"receiver\":\""), bytes32(bz[pos:pos + 13])
                );
            }
            (pd.receiver, pos) = parseString(bz, pos + 13);

            if (uint256(uint8(bz[pos + 2])) == CHAR_M) {
                if (bytes32(bz[pos:pos + 9]) != bytes32(",\"memo\":\"")) {
                    // solhint-disable-next-line max-line-length
                    revert IICS20Errors.ICS20JSONUnexpectedBytes(pos, bytes32(",\"memo\":\""), bytes32(bz[pos:pos + 9]));
                }
                (pd.memo, pos) = parseString(bz, pos + 9);
            }

            if (pos != bz.length - 1 || uint256(uint8(bz[pos])) != CHAR_CLOSING_BRACE) {
                revert IICS20Errors.ICS20JSONClosingBraceNotFound(pos, bz[pos]);
            }
        }

        return pd;
    }

    /// @notice parseUint256String parses `bz` from a position `pos` to produce a uint256.
    /// @param bz bytes
    /// @param pos position in the bytes
    /// @return ret uint256 value
    /// @return pos position after parsing
    function parseUint256String(bytes calldata bz, uint256 pos) private pure returns (uint256, uint256) {
        uint256 ret = 0;
        unchecked {
            for (; pos < bz.length; pos++) {
                uint256 c = uint256(uint8(bz[pos]));
                if (c < 48 || c > 57) {
                    break;
                }
                ret = ret * 10 + (c - 48);
            }
            if (pos >= bz.length || uint256(uint8(bz[pos])) != CHAR_DOUBLE_QUOTE) {
                revert IICS20Errors.ICS20JSONStringClosingDoubleQuoteNotFound(pos, bz[pos]);
            }
            return (ret, pos + 1);
        }
    }

    /// @notice parseString parses `bz` from a position `pos` to produce a string.
    /// @param bz bytes
    /// @param pos position in the bytes
    /// @return string value
    /// @return pos position after parsing
    function parseString(bytes calldata bz, uint256 pos) private pure returns (string memory, uint256) {
        unchecked {
            for (uint256 i = pos; i < bz.length; i++) {
                uint256 c = uint256(uint8(bz[i]));
                if (c == CHAR_DOUBLE_QUOTE) {
                    return (string(bz[pos:i]), i + 1);
                } else if (c == CHAR_BACKSLASH && i + 1 < bz.length) {
                    i++;
                    c = uint256(uint8(bz[i]));
                    if (
                        c != CHAR_DOUBLE_QUOTE && c != CHAR_SLASH && c != CHAR_BACKSLASH && c != CHAR_F && c != CHAR_R
                            && c != CHAR_N && c != CHAR_B && c != CHAR_T
                    ) {
                        revert IICS20Errors.ICS20JSONInvalidEscape(i, bz[i]);
                    }
                }
            }
        }
        revert IICS20Errors.ICS20JSONStringUnclosed(bz, pos);
    }

    /// @notice isEscapedJSONString checks if a string is escaped JSON.
    /// @param s string
    /// @return true if the string is escaped JSON
    function isEscapedJSONString(string calldata s) private pure returns (bool) {
        bytes memory bz = bytes(s);
        unchecked {
            for (uint256 i = 0; i < bz.length; i++) {
                uint256 c = uint256(uint8(bz[i]));
                if (c == CHAR_DOUBLE_QUOTE) {
                    return false;
                } else if (c == CHAR_BACKSLASH && i + 1 < bz.length) {
                    i++;
                    c = uint256(uint8(bz[i]));
                    if (
                        c != CHAR_DOUBLE_QUOTE && c != CHAR_SLASH && c != CHAR_BACKSLASH && c != CHAR_F && c != CHAR_R
                            && c != CHAR_N && c != CHAR_B && c != CHAR_T
                    ) {
                        return false;
                    }
                }
            }
        }
        return true;
    }

    /// @notice isEscapeNeededString checks if a string needs to be escaped.
    /// @param bz bytes
    /// @return true if the string needs to be escaped
    function isEscapeNeededString(bytes memory bz) private pure returns (bool) {
        unchecked {
            for (uint256 i = 0; i < bz.length; i++) {
                uint256 c = uint256(uint8(bz[i]));
                if (c == CHAR_DOUBLE_QUOTE) {
                    return true;
                }
            }
        }
        return false;
    }

    /// @notice hexStringToAddress converts a hex string to an address.
    /// @param addrHexString hex address string
    /// @return address value
    /// @return true if the conversion was successful
    function hexStringToAddress(string memory addrHexString) internal pure returns (address, bool) {
        bytes memory addrBytes = bytes(addrHexString);
        if (addrBytes.length != 42) {
            return (address(0), false);
        } else if (addrBytes[0] != "0" || addrBytes[1] != "x") {
            return (address(0), false);
        }
        uint256 addr = 0;
        unchecked {
            for (uint256 i = 2; i < 42; i++) {
                uint256 c = uint256(uint8(addrBytes[i]));
                if (c >= 48 && c <= 57) {
                    addr = addr * 16 + (c - 48);
                } else if (c >= 97 && c <= 102) {
                    addr = addr * 16 + (c - 87);
                } else if (c >= 65 && c <= 70) {
                    addr = addr * 16 + (c - 55);
                } else {
                    return (address(0), false);
                }
            }
        }
        return (address(uint160(addr)), true);
    }

    /// @notice slice returns a slice of the original bytes from `start` to `start + length`.
    /// @dev This is a copy from https://github.com/GNSPS/solidity-bytes-utils/blob/v0.8.0/contracts/BytesLib.sol
    /// @param _bytes bytes
    /// @param _start start index
    /// @param _length length
    /// @return sliced bytes
    function slice(bytes memory _bytes, uint256 _start, uint256 _length) internal pure returns (bytes memory) {
        if (_length + 31 < _length) {
            revert IICS20Errors.ICS20BytesSliceOverflow(_length);
        } else if (_start + _length > _bytes.length) {
            revert IICS20Errors.ICS20BytesSliceOutOfBounds(_bytes.length, _start, _start + _length);
        }

        bytes memory tempBytes;

        assembly {
            switch iszero(_length)
            case 0 {
                // Get a location of some free memory and store it in tempBytes as
                // Solidity does for memory variables.
                tempBytes := mload(0x40)

                // The first word of the slice result is potentially a partial
                // word read from the original array. To read it, we calculate
                // the length of that partial word and start copying that many
                // bytes into the array. The first word we copy will start with
                // data we don't care about, but the last `lengthmod` bytes will
                // land at the beginning of the contents of the new array. When
                // we're done copying, we overwrite the full first word with
                // the actual length of the slice.
                let lengthmod := and(_length, 31)

                // The multiplication in the next line is necessary
                // because when slicing multiples of 32 bytes (lengthmod == 0)
                // the following copy loop was copying the origin's length
                // and then ending prematurely not copying everything it should.
                let mc := add(add(tempBytes, lengthmod), mul(0x20, iszero(lengthmod)))
                let end := add(mc, _length)

                for {
                    // The multiplication in the next line has the same exact purpose
                    // as the one above.
                    let cc := add(add(add(_bytes, lengthmod), mul(0x20, iszero(lengthmod))), _start)
                } lt(mc, end) {
                    mc := add(mc, 0x20)
                    cc := add(cc, 0x20)
                } { mstore(mc, mload(cc)) }

                mstore(tempBytes, _length)

                //update free-memory pointer
                //allocating the array padded to 32 bytes like the compiler does now
                mstore(0x40, and(add(mc, 31), not(31)))
            }
            //if we want a zero-length slice let's just return a zero-length array
            default {
                tempBytes := mload(0x40)
                //zero out the 32 bytes slice we are about to return
                //we need to do it because Solidity does not garbage collect
                mstore(tempBytes, 0)

                mstore(0x40, add(tempBytes, 0x20))
            }
        }

        return tempBytes;
    }

    /// @notice equal returns true if two byte arrays are equal.
    /// @param a bytes
    /// @param b bytes
    /// @return true if the byte arrays are equal
    function equal(bytes memory a, bytes memory b) internal pure returns (bool) {
        // TODO: consider removing this function and using OpenZeppelin's Bytes library
        return keccak256(a) == keccak256(b);
    }

    /// @notice unwrapPacketData unmarshals packet data and converts addresses.
    /// @param data Packet data
    /// @return UnwrappedFungibleTokenPacketData
    function unwrapPacketData(bytes calldata data) internal pure returns (UnwrappedFungibleTokenPacketData memory) {
        ICS20Lib.PacketDataJSON memory packetData = ICS20Lib.unmarshalJSON(data);

        (address tokenContract, bool tokenContractConvertSuccess) = ICS20Lib.hexStringToAddress(packetData.denom);
        if (!tokenContractConvertSuccess) {
            revert IICS20Errors.ICS20InvalidTokenContract(packetData.denom);
        }

        (address sender, bool senderConvertSuccess) = ICS20Lib.hexStringToAddress(packetData.sender);
        if (!senderConvertSuccess) {
            revert IICS20Errors.ICS20InvalidSender(packetData.sender);
        }

        return UnwrappedFungibleTokenPacketData({
            erc20ContractAddress: tokenContract,
            amount: packetData.amount,
            sender: sender,
            receiver: packetData.receiver,
            memo: packetData.memo
        });
    }

    /// @notice errorAck returns an error acknowledgement.
    /// @param reason Error reason
    /// @return Error acknowledgement
    function errorAck(bytes memory reason) internal pure returns (bytes memory) {
        return abi.encodePacked("{\"error\":\"", reason, "\"}");
    }

    /// @notice getDenomPrefix returns the prefix for a denom.
    /// @param portId Port identifier
    /// @param channelId Channel identifier
    /// @return Denom prefix
    function getDenomPrefix(string calldata portId, string calldata channelId) internal pure returns (bytes memory) {
        return abi.encodePacked(portId, "/", channelId, "/");
    }
}
