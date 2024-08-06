// SPDX-License-Identifier: Apache-2.0
pragma solidity >=0.8.25;

// solhint-disable no-inline-assembly

import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import { IICS20Errors } from "../errors/IICS20Errors.sol";

// This library is mostly copied, with minor adjustments, from https://github.com/hyperledger-labs/yui-ibc-solidity
library ICS20Lib {
    /**
     * @dev PacketData is defined in
     * [ICS-20](https://github.com/cosmos/ibc/tree/main/spec/app/ics-020-fungible-token-transfer).
     */
    struct PacketDataJSON {
        string denom;
        string sender;
        string receiver;
        uint256 amount;
        string memo;
    }

    /// @notice Convenience type used after unmarshalling the packet data and converting addresses
    struct UnwrappedFungibleTokenPacketData {
        address erc20ContractAddress;
        uint256 amount;
        address sender;
        string receiver;
        string memo;
    }

    string public constant ICS20_VERSION = "ics20-1";

    bytes public constant SUCCESSFUL_ACKNOWLEDGEMENT_JSON = bytes("{\"result\":\"AQ==\"}");
    bytes public constant FAILED_ACKNOWLEDGEMENT_JSON = bytes("{\"error\":\"failed\"}");
    bytes32 internal constant KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON = keccak256(SUCCESSFUL_ACKNOWLEDGEMENT_JSON);

    uint256 private constant CHAR_DOUBLE_QUOTE = 0x22;
    uint256 private constant CHAR_SLASH = 0x2f;
    uint256 private constant CHAR_BACKSLASH = 0x5c;
    uint256 private constant CHAR_F = 0x66;
    uint256 private constant CHAR_R = 0x72;
    uint256 private constant CHAR_N = 0x6e;
    uint256 private constant CHAR_B = 0x62;
    uint256 private constant CHAR_T = 0x74;
    uint256 private constant CHAR_CLOSING_BRACE = 0x7d;
    uint256 private constant CHAR_M = 0x6d;

    bytes16 private constant HEX_DIGITS = "0123456789abcdef";

    /**
     * @dev marshalUnsafeJSON marshals PacketData into JSON bytes without escaping.
     *      `memo` field is omitted if it is empty.
     */
    function marshalUnsafeJSON(PacketDataJSON memory data) internal pure returns (bytes memory) {
        if (bytes(data.memo).length == 0) {
            return marshalJSON(data.denom, data.amount, data.sender, data.receiver);
        } else {
            return marshalJSON(data.denom, data.amount, data.sender, data.receiver, data.memo);
        }
    }

    /**
     * @dev marshalJSON marshals PacketData into JSON bytes with escaping.
     */
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

    /**
     * @dev marshalJSON marshals PacketData into JSON bytes with escaping.
     */
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

    /**
     * @dev unmarshalJSON unmarshals JSON bytes into PacketData.
     */
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

    /**
     * @dev parseUint256String parses `bz` from a position `pos` to produce a uint256.
     */
    function parseUint256String(bytes calldata bz, uint256 pos) internal pure returns (uint256, uint256) {
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

    /**
     * @dev parseString parses `bz` from a position `pos` to produce a string.
     */
    function parseString(bytes calldata bz, uint256 pos) internal pure returns (string memory, uint256) {
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

    function isEscapedJSONString(string calldata s) internal pure returns (bool) {
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

    function isEscapeNeededString(bytes memory bz) internal pure returns (bool) {
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

    /**
     * @dev hexStringToAddress converts a hex string to an address.
     */
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

    /**
     * @dev slice returns a slice of the original bytes from `start` to `start + length`.
     *      This is a copy from https://github.com/GNSPS/solidity-bytes-utils/blob/v0.8.0/contracts/BytesLib.sol
     */
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

    /**
     * @dev equal returns true if two byte arrays are equal.
     */
    function equal(bytes memory a, bytes memory b) internal pure returns (bool) {
        return keccak256(a) == keccak256(b);
    }

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

    function errorAck(bytes memory reason) internal pure returns (bytes memory) {
        return abi.encodePacked("{\"error\":\"", reason, "\"}");
    }

    function getDenomPrefix(string calldata port, string calldata channel) internal pure returns (bytes memory) {
        return abi.encodePacked(port, "/", channel, "/");
    }
}
