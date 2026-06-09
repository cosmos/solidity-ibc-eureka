// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ICometBFTClientErrors } from "../errors/ICometBFTClientErrors.sol";
import { ICometBFTMsgs } from "../msgs/ICometBFTMsgs.sol";
import { CometBFTProto } from "./CometBFTProto.sol";

/// @title CometBFT ICS-23 Proof Envelope
/// @notice Decodes and validates the native Solidity representation of the ICS-23 proof subset.
library CometBFTICS23 {
    uint8 private constant HASH_OP_NO_HASH = 0;
    uint8 private constant HASH_OP_SHA256 = 1;
    uint8 private constant LENGTH_OP_VAR_PROTO = 1;
    uint8 internal constant PROOF_TYPE_EXISTENCE = 1;
    uint8 internal constant PROOF_TYPE_NON_EXISTENCE = 2;
    uint256 private constant IAVL_CHILD_SIZE = 33;
    uint256 private constant IAVL_MIN_PREFIX_LENGTH = 4;
    uint256 private constant IAVL_MAX_PREFIX_WITH_LEFT_CHILD_LENGTH = 45;
    uint256 private constant TENDERMINT_CHILD_SIZE = 32;
    uint256 private constant TENDERMINT_MIN_PREFIX_LENGTH = 1;
    uint256 private constant TENDERMINT_MAX_PREFIX_WITH_LEFT_CHILD_LENGTH = 33;

    function verifyMembership(
        bytes32 root,
        bytes calldata proof,
        bytes[] calldata path,
        bytes calldata value
    )
        internal
        pure
    {
        ICometBFTMsgs.ICS23Proof memory decoded = decodeMembershipProof(proof, path, value);
        if (decoded.proofs.length != 2) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }

        bytes32 storeRoot = _calculateIavlRoot(decoded.proofs[0].existence, value);
        bytes32 appRoot = _calculateTendermintRoot(decoded.proofs[1].existence, abi.encodePacked(storeRoot));
        if (appRoot != root) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
    }

    function verifyNonMembership(bytes32 root, bytes calldata proof, bytes[] calldata path) internal pure {
        ICometBFTMsgs.ICS23Proof memory decoded = decodeNonMembershipProof(proof, path);
        if (decoded.proofs.length != 2) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }

        bytes32 storeRoot = _calculateIavlNonMembershipRoot(decoded.proofs[0].nonExistence);
        bytes32 appRoot = _calculateTendermintRoot(decoded.proofs[1].existence, abi.encodePacked(storeRoot));
        if (appRoot != root) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
    }

    function decodeMembershipProof(
        bytes calldata proof,
        bytes[] calldata path,
        bytes calldata value
    )
        internal
        pure
        returns (ICometBFTMsgs.ICS23Proof memory decoded)
    {
        if (value.length == 0) {
            revert ICometBFTClientErrors.EmptyMembershipValue();
        }
        decoded = _decodeProof(proof, path);
        for (uint256 i = 0; i < decoded.proofs.length; ++i) {
            ICometBFTMsgs.ICS23CommitmentProof memory commitmentProof = decoded.proofs[i];
            _validateSupportedProofType(commitmentProof.proofType);
            if (commitmentProof.proofType != PROOF_TYPE_EXISTENCE) {
                revert ICometBFTClientErrors.InvalidICS23Proof();
            }
            if (!_isEmptyNonExistenceProof(commitmentProof.nonExistence)) {
                revert ICometBFTClientErrors.InvalidICS23Proof();
            }
            _validateExistenceProof(commitmentProof.existence);
            _validateProofKey(commitmentProof.existence.key, path, i);
            if (i == 0 && keccak256(commitmentProof.existence.value) != keccak256(value)) {
                revert ICometBFTClientErrors.InvalidICS23Proof();
            }
        }
    }

    function decodeNonMembershipProof(
        bytes calldata proof,
        bytes[] calldata path
    )
        internal
        pure
        returns (ICometBFTMsgs.ICS23Proof memory decoded)
    {
        decoded = _decodeProof(proof, path);
        for (uint256 i = 0; i < decoded.proofs.length; ++i) {
            ICometBFTMsgs.ICS23CommitmentProof memory commitmentProof = decoded.proofs[i];
            _validateSupportedProofType(commitmentProof.proofType);
            if (i == 0) {
                if (commitmentProof.proofType != PROOF_TYPE_NON_EXISTENCE) {
                    revert ICometBFTClientErrors.InvalidICS23Proof();
                }
                if (!_isEmptyExistenceProof(commitmentProof.existence)) {
                    revert ICometBFTClientErrors.InvalidICS23Proof();
                }
                _validateNonExistenceProof(commitmentProof.nonExistence);
                _validateProofKey(commitmentProof.nonExistence.key, path, i);
            } else {
                if (commitmentProof.proofType != PROOF_TYPE_EXISTENCE) {
                    revert ICometBFTClientErrors.InvalidICS23Proof();
                }
                if (!_isEmptyNonExistenceProof(commitmentProof.nonExistence)) {
                    revert ICometBFTClientErrors.InvalidICS23Proof();
                }
                _validateExistenceProof(commitmentProof.existence);
                _validateProofKey(commitmentProof.existence.key, path, i);
            }
        }
    }

    function _decodeProof(
        bytes calldata proof,
        bytes[] calldata path
    )
        private
        pure
        returns (ICometBFTMsgs.ICS23Proof memory decoded)
    {
        if (proof.length == 0 || path.length == 0) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
        decoded = abi.decode(proof, (ICometBFTMsgs.ICS23Proof));
        if (decoded.proofs.length != path.length) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
    }

    function _validateSupportedProofType(uint8 proofType) private pure {
        if (proofType != PROOF_TYPE_EXISTENCE && proofType != PROOF_TYPE_NON_EXISTENCE) {
            revert ICometBFTClientErrors.UnsupportedICS23ProofType(proofType);
        }
    }

    function _validateExistenceProof(ICometBFTMsgs.ICS23ExistenceProof memory proof) private pure {
        if (proof.key.length == 0 || !proof.hasLeaf) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
    }

    function _validateNonExistenceProof(ICometBFTMsgs.ICS23NonExistenceProof memory proof) private pure {
        if (proof.key.length == 0 || (!proof.left.exists && !proof.right.exists)) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
        if (proof.left.exists) {
            _validateExistenceProof(proof.left.proof);
        } else if (!_isEmptyOptionalExistenceProof(proof.left)) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
        if (proof.right.exists) {
            _validateExistenceProof(proof.right.proof);
        } else if (!_isEmptyOptionalExistenceProof(proof.right)) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
    }

    function _validateProofKey(bytes memory proofKey, bytes[] calldata path, uint256 proofIndex) private pure {
        bytes calldata pathKey = path[path.length - 1 - proofIndex];
        if (proofKey.length != pathKey.length || keccak256(proofKey) != keccak256(pathKey)) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
    }

    function _calculateIavlRoot(
        ICometBFTMsgs.ICS23ExistenceProof memory proof,
        bytes memory expectedValue
    )
        private
        pure
        returns (bytes32)
    {
        return _calculateExistenceRoot(proof, expectedValue, true);
    }

    function _calculateTendermintRoot(
        ICometBFTMsgs.ICS23ExistenceProof memory proof,
        bytes memory expectedValue
    )
        private
        pure
        returns (bytes32)
    {
        return _calculateExistenceRoot(proof, expectedValue, false);
    }

    function _calculateIavlNonMembershipRoot(ICometBFTMsgs.ICS23NonExistenceProof memory proof)
        private
        pure
        returns (bytes32 storeRoot)
    {
        bool hasRoot;
        if (proof.left.exists) {
            if (_compareBytes(proof.left.proof.key, proof.key) >= 0) {
                revert ICometBFTClientErrors.InvalidICS23Proof();
            }
            storeRoot = _calculateIavlRoot(proof.left.proof, proof.left.proof.value);
            hasRoot = true;
        }
        if (proof.right.exists) {
            if (_compareBytes(proof.key, proof.right.proof.key) >= 0) {
                revert ICometBFTClientErrors.InvalidICS23Proof();
            }
            bytes32 rightRoot = _calculateIavlRoot(proof.right.proof, proof.right.proof.value);
            if (hasRoot && rightRoot != storeRoot) {
                revert ICometBFTClientErrors.InvalidICS23Proof();
            }
            storeRoot = rightRoot;
            hasRoot = true;
        }
        if (!hasRoot) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }

        _validateIavlNonMembershipNeighbors(proof);
    }

    function _calculateExistenceRoot(
        ICometBFTMsgs.ICS23ExistenceProof memory proof,
        bytes memory expectedValue,
        bool iavlSpec
    )
        private
        pure
        returns (bytes32 root)
    {
        if (proof.value.length != expectedValue.length || keccak256(proof.value) != keccak256(expectedValue)) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
        _validateLeafSpec(proof.leaf, iavlSpec);

        root = _applyLeaf(proof.leaf, proof.key, proof.value);
        for (uint256 i = 0; i < proof.path.length; ++i) {
            _validateInnerSpec(proof.path[i], i + 1, iavlSpec);
            root = sha256(abi.encodePacked(proof.path[i].prefix, root, proof.path[i].suffix));
        }
    }

    function _validateLeafSpec(ICometBFTMsgs.ICS23LeafOp memory leaf, bool iavlSpec) private pure {
        if (
            leaf.hash != HASH_OP_SHA256 || leaf.prehashKey != HASH_OP_NO_HASH || leaf.prehashValue != HASH_OP_SHA256
                || leaf.length != LENGTH_OP_VAR_PROTO || !_hasPrefix(hex"00", leaf.prefix)
        ) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
        if (iavlSpec && _remainingIavlPrefixBytes(leaf.prefix, 0) != 0) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
    }

    function _validateInnerSpec(
        ICometBFTMsgs.ICS23InnerOp memory inner,
        uint256 minHeight,
        bool iavlSpec
    )
        private
        pure
    {
        if (inner.hash != HASH_OP_SHA256 || _hasPrefix(hex"00", inner.prefix)) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }

        if (iavlSpec) {
            if (
                inner.prefix.length < IAVL_MIN_PREFIX_LENGTH
                    || inner.prefix.length > IAVL_MAX_PREFIX_WITH_LEFT_CHILD_LENGTH
                    || inner.suffix.length % IAVL_CHILD_SIZE != 0
            ) {
                revert ICometBFTClientErrors.InvalidICS23Proof();
            }
            uint256 remaining = _remainingIavlPrefixBytes(inner.prefix, minHeight);
            if (remaining != 1 && remaining != 34) {
                revert ICometBFTClientErrors.InvalidICS23Proof();
            }
        } else if (
            inner.prefix.length < TENDERMINT_MIN_PREFIX_LENGTH
                || inner.prefix.length > TENDERMINT_MAX_PREFIX_WITH_LEFT_CHILD_LENGTH
                || inner.suffix.length % TENDERMINT_CHILD_SIZE != 0
        ) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
    }

    function _applyLeaf(
        ICometBFTMsgs.ICS23LeafOp memory leaf,
        bytes memory key,
        bytes memory value
    )
        private
        pure
        returns (bytes32)
    {
        bytes32 valueHash = sha256(value);
        return sha256(
            abi.encodePacked(
                leaf.prefix, CometBFTProto.encodeVarint(key.length), key, CometBFTProto.encodeVarint(32), valueHash
            )
        );
    }

    function _remainingIavlPrefixBytes(bytes memory prefix, uint256 minHeight) private pure returns (uint256) {
        uint256 cursor;
        uint256 height;
        (height, cursor) = _readNonNegativeSignedVarint(prefix, cursor);
        if (height < minHeight) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }

        uint256 size;
        (size, cursor) = _readNonNegativeSignedVarint(prefix, cursor);
        size;

        uint256 version;
        (version, cursor) = _readNonNegativeSignedVarint(prefix, cursor);
        version;

        return prefix.length - cursor;
    }

    function _readNonNegativeSignedVarint(
        bytes memory data,
        uint256 cursor
    )
        private
        pure
        returns (uint256 value, uint256 nextCursor)
    {
        uint256 raw;
        uint256 shift;
        while (cursor < data.length && shift < 64) {
            uint8 b = uint8(data[cursor]);
            ++cursor;
            raw |= uint256(b & 0x7f) << shift;
            if (b < 0x80) {
                if (raw & 1 != 0) {
                    revert ICometBFTClientErrors.InvalidICS23Proof();
                }
                return (raw >> 1, cursor);
            }
            shift += 7;
        }
        revert ICometBFTClientErrors.InvalidICS23Proof();
    }

    function _validateIavlNonMembershipNeighbors(ICometBFTMsgs.ICS23NonExistenceProof memory proof) private pure {
        if (proof.left.exists && proof.right.exists) {
            _ensureIavlLeftNeighbor(proof.left.proof.path, proof.right.proof.path);
        } else if (proof.left.exists) {
            _ensureIavlRightMost(proof.left.proof.path);
        } else if (proof.right.exists) {
            _ensureIavlLeftMost(proof.right.proof.path);
        }
    }

    function _ensureIavlLeftNeighbor(
        ICometBFTMsgs.ICS23InnerOp[] memory left,
        ICometBFTMsgs.ICS23InnerOp[] memory right
    )
        private
        pure
    {
        if (left.length == 0 || right.length == 0) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }

        uint256 leftIndex = left.length - 1;
        uint256 rightIndex = right.length - 1;
        while (_innerOpsEqual(left[leftIndex], right[rightIndex]) && leftIndex > 0 && rightIndex > 0) {
            --leftIndex;
            --rightIndex;
        }

        if (!_isIavlLeftStep(left[leftIndex], right[rightIndex])) {
            revert ICometBFTClientErrors.InvalidICS23Proof();
        }
        _ensureIavlRightMostPrefix(left, leftIndex);
        _ensureIavlLeftMostPrefix(right, rightIndex);
    }

    function _ensureIavlLeftMost(ICometBFTMsgs.ICS23InnerOp[] memory path) private pure {
        _ensureIavlLeftMostPrefix(path, path.length);
    }

    function _ensureIavlRightMost(ICometBFTMsgs.ICS23InnerOp[] memory path) private pure {
        _ensureIavlRightMostPrefix(path, path.length);
    }

    function _ensureIavlLeftMostPrefix(ICometBFTMsgs.ICS23InnerOp[] memory path, uint256 length) private pure {
        for (uint256 i = 0; i < length; ++i) {
            if (!_hasIavlPadding(path[i], 0)) {
                revert ICometBFTClientErrors.InvalidICS23Proof();
            }
        }
    }

    function _ensureIavlRightMostPrefix(ICometBFTMsgs.ICS23InnerOp[] memory path, uint256 length) private pure {
        for (uint256 i = 0; i < length; ++i) {
            if (!_hasIavlPadding(path[i], 1)) {
                revert ICometBFTClientErrors.InvalidICS23Proof();
            }
        }
    }

    function _isIavlLeftStep(
        ICometBFTMsgs.ICS23InnerOp memory left,
        ICometBFTMsgs.ICS23InnerOp memory right
    )
        private
        pure
        returns (bool)
    {
        return _hasIavlPadding(left, 0) && _hasIavlPadding(right, 1);
    }

    function _hasIavlPadding(ICometBFTMsgs.ICS23InnerOp memory op, uint256 branch) private pure returns (bool) {
        if (branch == 0) {
            return
                op.prefix.length >= IAVL_MIN_PREFIX_LENGTH && op.prefix.length <= 12
                    && op.suffix.length == IAVL_CHILD_SIZE;
        }
        return
            op.prefix.length >= 37 && op.prefix.length <= IAVL_MAX_PREFIX_WITH_LEFT_CHILD_LENGTH
                && op.suffix.length == 0;
    }

    function _innerOpsEqual(
        ICometBFTMsgs.ICS23InnerOp memory left,
        ICometBFTMsgs.ICS23InnerOp memory right
    )
        private
        pure
        returns (bool)
    {
        return left.hash == right.hash && keccak256(left.prefix) == keccak256(right.prefix)
            && keccak256(left.suffix) == keccak256(right.suffix);
    }

    function _compareBytes(bytes memory left, bytes memory right) private pure returns (int256) {
        uint256 minLength = left.length < right.length ? left.length : right.length;
        for (uint256 i = 0; i < minLength; ++i) {
            if (uint8(left[i]) < uint8(right[i])) {
                return -1;
            }
            if (uint8(left[i]) > uint8(right[i])) {
                return 1;
            }
        }
        if (left.length < right.length) {
            return -1;
        }
        if (left.length > right.length) {
            return 1;
        }
        return 0;
    }

    function _hasPrefix(bytes memory prefix, bytes memory data) private pure returns (bool) {
        if (prefix.length > data.length) {
            return false;
        }
        for (uint256 i = 0; i < prefix.length; ++i) {
            if (prefix[i] != data[i]) {
                return false;
            }
        }
        return true;
    }

    function _isEmptyExistenceProof(ICometBFTMsgs.ICS23ExistenceProof memory proof) private pure returns (bool) {
        return proof.key.length == 0 && proof.value.length == 0 && !proof.hasLeaf && _isEmptyLeafOp(proof.leaf)
            && proof.path.length == 0;
    }

    function _isEmptyNonExistenceProof(ICometBFTMsgs.ICS23NonExistenceProof memory proof) private pure returns (bool) {
        return proof.key.length == 0 && _isEmptyOptionalExistenceProof(proof.left)
            && _isEmptyOptionalExistenceProof(proof.right);
    }

    function _isEmptyOptionalExistenceProof(ICometBFTMsgs.ICS23OptionalExistenceProof memory proof)
        private
        pure
        returns (bool)
    {
        return !proof.exists && _isEmptyExistenceProof(proof.proof);
    }

    function _isEmptyLeafOp(ICometBFTMsgs.ICS23LeafOp memory leaf) private pure returns (bool) {
        return
            leaf.hash == 0 && leaf.prehashKey == 0 && leaf.prehashValue == 0 && leaf.length == 0
                && leaf.prefix.length == 0;
    }
}
