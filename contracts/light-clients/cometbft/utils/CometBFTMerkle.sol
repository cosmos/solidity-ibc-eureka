// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title CometBFT Merkle Hashing
/// @notice RFC6962-style SHA-256 Merkle hashing used by CometBFT.
library CometBFTMerkle {
    function hashFromByteSlices(bytes[] memory items) internal pure returns (bytes32) {
        return _hashFromByteSlices(items, 0, items.length);
    }

    function leafHash(bytes memory leaf) internal pure returns (bytes32) {
        return sha256(abi.encodePacked(bytes1(0x00), leaf));
    }

    function innerHash(bytes32 left, bytes32 right) internal pure returns (bytes32) {
        return sha256(abi.encodePacked(bytes1(0x01), left, right));
    }

    function _hashFromByteSlices(bytes[] memory items, uint256 start, uint256 end) private pure returns (bytes32) {
        uint256 len = end - start;
        if (len == 0) {
            return sha256("");
        }
        if (len == 1) {
            return leafHash(items[start]);
        }

        uint256 split = start + _splitPoint(len);
        return innerHash(_hashFromByteSlices(items, start, split), _hashFromByteSlices(items, split, end));
    }

    function _splitPoint(uint256 length) private pure returns (uint256) {
        uint256 k = 1;
        while ((k << 1) < length) {
            k <<= 1;
        }
        return k == length ? k >> 1 : k;
    }
}
