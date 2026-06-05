// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ICometBFTMsgs } from "../msgs/ICometBFTMsgs.sol";
import { CometBFTMerkle } from "./CometBFTMerkle.sol";

/// @title CometBFT Protobuf Encoders
/// @notice Minimal protobuf encoders needed for native CometBFT header and vote verification.
library CometBFTProto {
    uint8 internal constant BLOCK_ID_FLAG_ABSENT = 1;
    uint8 internal constant BLOCK_ID_FLAG_COMMIT = 2;
    uint8 internal constant BLOCK_ID_FLAG_NIL = 3;
    uint8 internal constant PRECOMMIT_TYPE = 2;

    function headerHash(ICometBFTMsgs.Header memory header_) internal pure returns (bytes32) {
        bytes[] memory leaves = new bytes[](14);
        leaves[0] = encodeConsensusVersion(header_.versionBlock, header_.versionApp);
        leaves[1] = wrapString(header_.chainId);
        leaves[2] = wrapInt64(header_.height);
        leaves[3] = encodeTimestamp(header_.timeSeconds, header_.timeNanos);
        leaves[4] = encodeBlockID(header_.lastBlockId);
        leaves[5] = wrapBytes32(header_.lastCommitHash);
        leaves[6] = wrapBytes32(header_.dataHash);
        leaves[7] = wrapBytes32(header_.validatorsHash);
        leaves[8] = wrapBytes32(header_.nextValidatorsHash);
        leaves[9] = wrapBytes32(header_.consensusHash);
        leaves[10] = wrapBytes32(header_.appHash);
        leaves[11] = wrapBytes32(header_.lastResultsHash);
        leaves[12] = wrapBytes32(header_.evidenceHash);
        leaves[13] = wrapAddress(header_.proposerAddress);
        return CometBFTMerkle.hashFromByteSlices(leaves);
    }

    function validatorSetHash(ICometBFTMsgs.Validator[] memory validators) internal pure returns (bytes32) {
        bytes[] memory leaves = new bytes[](validators.length);
        for (uint256 i = 0; i < validators.length; ++i) {
            leaves[i] = encodeSimpleValidator(validators[i]);
        }
        return CometBFTMerkle.hashFromByteSlices(leaves);
    }

    function voteSignBytes(
        string memory chainId,
        ICometBFTMsgs.Commit memory commit,
        ICometBFTMsgs.CommitSig memory commitSig
    )
        internal
        pure
        returns (bytes memory)
    {
        bool includeBlockID = commitSig.blockIdFlag == BLOCK_ID_FLAG_COMMIT;
        bytes memory vote = encodeCanonicalVote(
            PRECOMMIT_TYPE,
            commit.height,
            commit.round,
            includeBlockID,
            commit.blockId,
            commitSig.timestampSeconds,
            commitSig.timestampNanos,
            chainId
        );
        return abi.encodePacked(encodeVarint(vote.length), vote);
    }

    function encodeSimpleValidator(ICometBFTMsgs.Validator memory validator) internal pure returns (bytes memory) {
        bytes memory pubKey = abi.encodePacked(bytes1(0x2a), encodeVarint(validator.pubKey.length), validator.pubKey);
        return abi.encodePacked(
            bytes1(0x0a),
            encodeVarint(pubKey.length),
            pubKey,
            validator.votingPower == 0 ? bytes("") : abi.encodePacked(bytes1(0x10), encodeVarint(validator.votingPower))
        );
    }

    function encodeCanonicalVote(
        uint8 msgType,
        uint64 height,
        uint32 round,
        bool includeBlockID,
        ICometBFTMsgs.BlockID memory blockID,
        uint64 timestampSeconds,
        uint32 timestampNanos,
        string memory chainId
    )
        internal
        pure
        returns (bytes memory)
    {
        bytes memory blockIDField = "";
        if (includeBlockID) {
            bytes memory blockIDBytes = encodeCanonicalBlockID(blockID);
            blockIDField = abi.encodePacked(bytes1(0x22), encodeVarint(blockIDBytes.length), blockIDBytes);
        }

        bytes memory timestampBytes = encodeTimestamp(timestampSeconds, timestampNanos);
        return abi.encodePacked(
            msgType == 0 ? bytes("") : abi.encodePacked(bytes1(0x08), encodeVarint(msgType)),
            height == 0 ? bytes("") : abi.encodePacked(bytes1(0x11), encodeFixed64(height)),
            round == 0 ? bytes("") : abi.encodePacked(bytes1(0x19), encodeFixed64(round)),
            blockIDField,
            bytes1(0x2a),
            encodeVarint(timestampBytes.length),
            timestampBytes,
            bytes(chainId).length == 0
                ? bytes("")
                : abi.encodePacked(bytes1(0x32), encodeVarint(bytes(chainId).length), bytes(chainId))
        );
    }

    function encodeConsensusVersion(uint64 blockVersion, uint64 appVersion) internal pure returns (bytes memory) {
        return abi.encodePacked(
            blockVersion == 0 ? bytes("") : abi.encodePacked(bytes1(0x08), encodeVarint(blockVersion)),
            appVersion == 0 ? bytes("") : abi.encodePacked(bytes1(0x10), encodeVarint(appVersion))
        );
    }

    function encodeBlockID(ICometBFTMsgs.BlockID memory blockID) internal pure returns (bytes memory) {
        bytes memory partSetHeader = encodePartSetHeader(blockID.partSetHeader);
        return abi.encodePacked(
            blockID.hash == bytes32(0) ? bytes("") : abi.encodePacked(bytes1(0x0a), encodeVarint(32), blockID.hash),
            bytes1(0x12),
            encodeVarint(partSetHeader.length),
            partSetHeader
        );
    }

    function encodeCanonicalBlockID(ICometBFTMsgs.BlockID memory blockID) internal pure returns (bytes memory) {
        bytes memory partSetHeader = encodePartSetHeader(blockID.partSetHeader);
        return abi.encodePacked(
            blockID.hash == bytes32(0) ? bytes("") : abi.encodePacked(bytes1(0x0a), encodeVarint(32), blockID.hash),
            bytes1(0x12),
            encodeVarint(partSetHeader.length),
            partSetHeader
        );
    }

    function encodePartSetHeader(ICometBFTMsgs.PartSetHeader memory partSetHeader)
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(
            partSetHeader.total == 0 ? bytes("") : abi.encodePacked(bytes1(0x08), encodeVarint(partSetHeader.total)),
            partSetHeader.hash == bytes32(0)
                ? bytes("")
                : abi.encodePacked(bytes1(0x12), encodeVarint(32), partSetHeader.hash)
        );
    }

    function encodeTimestamp(uint64 seconds_, uint32 nanos) internal pure returns (bytes memory) {
        return abi.encodePacked(
            seconds_ == 0 ? bytes("") : abi.encodePacked(bytes1(0x08), encodeVarint(seconds_)),
            nanos == 0 ? bytes("") : abi.encodePacked(bytes1(0x10), encodeVarint(uint64(nanos)))
        );
    }

    function wrapString(string memory value) internal pure returns (bytes memory) {
        bytes memory raw = bytes(value);
        if (raw.length == 0) {
            return "";
        }
        return abi.encodePacked(bytes1(0x0a), encodeVarint(raw.length), raw);
    }

    function wrapInt64(uint64 value) internal pure returns (bytes memory) {
        if (value == 0) {
            return "";
        }
        return abi.encodePacked(bytes1(0x08), encodeVarint(value));
    }

    function wrapBytes32(bytes32 value) internal pure returns (bytes memory) {
        if (value == bytes32(0)) {
            return "";
        }
        return abi.encodePacked(bytes1(0x0a), encodeVarint(32), value);
    }

    function wrapAddress(address value) internal pure returns (bytes memory) {
        return abi.encodePacked(bytes1(0x0a), encodeVarint(20), abi.encodePacked(value));
    }

    function encodeFixed64(uint64 value) internal pure returns (bytes memory out) {
        out = new bytes(8);
        for (uint256 i = 0; i < 8; ++i) {
            out[i] = bytes1(uint8(value >> (i * 8)));
        }
    }

    function encodeVarint(uint256 value) internal pure returns (bytes memory out) {
        uint256 tmp = value;
        uint256 len = 1;
        while (tmp >= 0x80) {
            tmp >>= 7;
            ++len;
        }

        out = new bytes(len);
        for (uint256 i = 0; i < len; ++i) {
            uint8 b = uint8(value & 0x7f);
            value >>= 7;
            if (value != 0) {
                b |= 0x80;
            }
            out[i] = bytes1(b);
        }
    }
}
