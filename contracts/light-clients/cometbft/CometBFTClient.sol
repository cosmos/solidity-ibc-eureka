// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { AccessControl } from "@openzeppelin-contracts/access/AccessControl.sol";

import { ILightClient } from "../../interfaces/ILightClient.sol";
import { ILightClientMsgs } from "../../msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../msgs/IICS02ClientMsgs.sol";
import { ICometBFTClientErrors } from "./errors/ICometBFTClientErrors.sol";
import { ICometBFTMsgs } from "./msgs/ICometBFTMsgs.sol";
import { CometBFTECDSA } from "./utils/CometBFTECDSA.sol";
import { CometBFTProto } from "./utils/CometBFTProto.sol";

/// @title Native CometBFT Light Client
/// @notice Native adjacent-update CometBFT light client for secp256k1eth validator sets.
contract CometBFTClient is ILightClient, ICometBFTClientErrors, AccessControl {
    bytes32 public constant PROOF_SUBMITTER_ROLE = keccak256("PROOF_SUBMITTER_ROLE");
    uint8 private constant BLOCK_ID_FLAG_ABSENT = 1;
    uint8 private constant BLOCK_ID_FLAG_COMMIT = 2;
    uint8 private constant BLOCK_ID_FLAG_NIL = 3;
    uint256 private constant SECP256K1_P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F;

    ICometBFTMsgs.ClientState private clientState;
    mapping(uint64 revisionHeight => bytes32 consensusStateHash) private consensusStateHashes;

    constructor(
        ICometBFTMsgs.ClientState memory initialClientState,
        ICometBFTMsgs.ConsensusState memory initialConsensusState,
        address roleManager
    ) {
        require(bytes(initialClientState.chainId).length > 0, InvalidValidatorSet());
        require(initialClientState.trustLevel.denominator > 0, InvalidValidatorSet());
        require(
            initialClientState.trustLevel.numerator * 3 >= initialClientState.trustLevel.denominator,
            InvalidValidatorSet()
        );
        require(
            initialClientState.trustLevel.numerator <= initialClientState.trustLevel.denominator, InvalidValidatorSet()
        );

        clientState = initialClientState;
        consensusStateHashes[initialClientState.latestHeight.revisionHeight] =
            _consensusStateHash(initialConsensusState);

        _grantRole(DEFAULT_ADMIN_ROLE, roleManager);
        _grantRole(PROOF_SUBMITTER_ROLE, roleManager);
    }

    /// @inheritdoc ILightClient
    function getClientState() external view returns (bytes memory) {
        return abi.encode(clientState);
    }

    /// @notice Returns the stored consensus-state hash for a revision height.
    function getConsensusStateHash(uint64 revisionHeight) public view returns (bytes32) {
        bytes32 hash = consensusStateHashes[revisionHeight];
        if (hash == bytes32(0)) {
            revert ConsensusStateNotFound(revisionHeight);
        }
        return hash;
    }

    /// @inheritdoc ILightClient
    function updateClient(bytes calldata updateMsg)
        external
        notFrozen
        onlyProofSubmitter
        returns (ILightClientMsgs.UpdateResult)
    {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = abi.decode(updateMsg, (ICometBFTMsgs.MsgUpdateClient));

        _validateTrustedConsensusState(msg_.trustedHeight.revisionHeight, msg_.trustedConsensusState);
        _validateHeaderAndValidatorSet(msg_);
        _verifyCommit(msg_.header.chainId, msg_.commit, msg_.validators);

        ICometBFTMsgs.ConsensusState memory newConsensusState = ICometBFTMsgs.ConsensusState({
            timestamp: _timestampNanos(msg_.header.timeSeconds, msg_.header.timeNanos),
            root: msg_.header.appHash,
            nextValidatorsHash: msg_.header.nextValidatorsHash
        });

        ILightClientMsgs.UpdateResult result =
            _checkUpdateResult(msg_.trustedConsensusState, msg_.header.height, newConsensusState);
        if (result == ILightClientMsgs.UpdateResult.Update) {
            consensusStateHashes[msg_.header.height] = _consensusStateHash(newConsensusState);
            if (msg_.header.height > clientState.latestHeight.revisionHeight) {
                clientState.latestHeight = IICS02ClientMsgs.Height({
                    revisionNumber: clientState.latestHeight.revisionNumber, revisionHeight: msg_.header.height
                });
            }
        } else if (result == ILightClientMsgs.UpdateResult.Misbehaviour) {
            clientState.isFrozen = true;
        }

        return result;
    }

    /// @inheritdoc ILightClient
    function verifyMembership(ILightClientMsgs.MsgVerifyMembership calldata) external pure returns (uint256) {
        revert FeatureNotSupported();
    }

    /// @inheritdoc ILightClient
    function verifyNonMembership(ILightClientMsgs.MsgVerifyNonMembership calldata) external pure returns (uint256) {
        revert FeatureNotSupported();
    }

    /// @inheritdoc ILightClient
    function misbehaviour(bytes calldata) external pure {
        revert FeatureNotSupported();
    }

    function _validateTrustedConsensusState(
        uint64 trustedHeight,
        ICometBFTMsgs.ConsensusState memory trustedConsensusState
    )
        private
        view
    {
        bytes32 expected = getConsensusStateHash(trustedHeight);
        bytes32 actual = _consensusStateHash(trustedConsensusState);
        if (actual != expected) {
            revert ConsensusStateHashMismatch(expected, actual);
        }
    }

    function _validateHeaderAndValidatorSet(ICometBFTMsgs.MsgUpdateClient memory msg_) private view {
        ICometBFTMsgs.Header memory header_ = msg_.header;
        ICometBFTMsgs.ConsensusState memory trustedConsensusState = msg_.trustedConsensusState;

        if (keccak256(bytes(header_.chainId)) != keccak256(bytes(clientState.chainId))) {
            revert ChainIdMismatch(clientState.chainId, header_.chainId);
        }
        if (header_.height != msg_.trustedHeight.revisionHeight + 1) {
            revert UnsupportedNonAdjacentUpdate(msg_.trustedHeight.revisionHeight, header_.height);
        }

        uint128 headerTime = _timestampNanos(header_.timeSeconds, header_.timeNanos);
        if (headerTime <= trustedConsensusState.timestamp) {
            revert HeaderTimeNotIncreasing(trustedConsensusState.timestamp, headerTime);
        }

        uint256 trustedSeconds = uint256(trustedConsensusState.timestamp) / 1e9;
        uint256 trustedExpiresAt = trustedSeconds + clientState.trustingPeriod;
        if (trustedExpiresAt <= block.timestamp) {
            revert TrustedConsensusStateExpired(trustedExpiresAt, block.timestamp);
        }
        if (header_.timeSeconds >= block.timestamp + clientState.maxClockDrift) {
            revert HeaderFromFuture(header_.timeSeconds, block.timestamp, clientState.maxClockDrift);
        }

        _validateValidatorOrdering(msg_.validators);
        bytes32 validatorsHash = CometBFTProto.validatorSetHash(msg_.validators);
        if (validatorsHash != header_.validatorsHash) {
            revert ValidatorSetHashMismatch(header_.validatorsHash, validatorsHash);
        }
        if (header_.validatorsHash != trustedConsensusState.nextValidatorsHash) {
            revert AdjacentValidatorHashMismatch(trustedConsensusState.nextValidatorsHash, header_.validatorsHash);
        }

        bytes32 headerHash = CometBFTProto.headerHash(header_);
        if (headerHash != msg_.commit.blockId.hash) {
            revert HeaderCommitHashMismatch(headerHash, msg_.commit.blockId.hash);
        }
        if (msg_.commit.height != header_.height) {
            revert InvalidCommitHeight(header_.height, msg_.commit.height);
        }
        if (msg_.commit.blockId.partSetHeader.total == 0 || msg_.commit.blockId.partSetHeader.hash == bytes32(0)) {
            revert InvalidCommitBlockID();
        }
    }

    function _verifyCommit(
        string memory chainId,
        ICometBFTMsgs.Commit memory commit,
        ICometBFTMsgs.Validator[] memory validators
    )
        private
        pure
    {
        if (commit.signatures.length != validators.length) {
            revert InvalidCommitSignaturesLength(validators.length, commit.signatures.length);
        }

        uint256 totalVotingPower;
        for (uint256 i = 0; i < validators.length; ++i) {
            _validateCommitSigBasic(i, commit.signatures[i]);
            totalVotingPower += validators[i].votingPower;
        }
        uint256 votingPowerNeeded = totalVotingPower * 2 / 3;
        uint256 signedVotingPower;

        for (uint256 i = 0; i < commit.signatures.length; ++i) {
            ICometBFTMsgs.CommitSig memory sig = commit.signatures[i];
            if (sig.blockIdFlag == BLOCK_ID_FLAG_ABSENT) {
                continue;
            }

            address expected = _validatorAddress(i, validators[i]);
            if (sig.validatorAddress != expected) {
                revert ValidatorAddressMismatch(i, expected, sig.validatorAddress);
            }

            bytes memory signBytes = CometBFTProto.voteSignBytes(chainId, commit, sig);
            address recovered = CometBFTECDSA.recover(keccak256(signBytes), sig.signature);
            if (recovered != expected) {
                revert SignatureSignerMismatch(i, expected, recovered);
            }

            if (sig.blockIdFlag == BLOCK_ID_FLAG_COMMIT) {
                signedVotingPower += validators[i].votingPower;
                if (signedVotingPower > votingPowerNeeded) {
                    return;
                }
            }
        }

        revert NotEnoughVotingPower(signedVotingPower, votingPowerNeeded);
    }

    function _validateCommitSigBasic(uint256 index, ICometBFTMsgs.CommitSig memory sig) private pure {
        if (sig.blockIdFlag == BLOCK_ID_FLAG_ABSENT) {
            if (
                sig.validatorAddress != address(0) || sig.timestampSeconds != 0 || sig.timestampNanos != 0
                    || sig.signature.length != 0
            ) {
                revert InvalidAbsentCommitSignature(index);
            }
            return;
        }
        if (sig.blockIdFlag != BLOCK_ID_FLAG_COMMIT && sig.blockIdFlag != BLOCK_ID_FLAG_NIL) {
            revert InvalidBlockIDFlag(sig.blockIdFlag);
        }
    }

    function _validateValidatorOrdering(ICometBFTMsgs.Validator[] memory validators) private pure {
        if (validators.length == 0) {
            revert InvalidValidatorSet();
        }

        for (uint256 i = 0; i < validators.length; ++i) {
            if (validators[i].votingPower == 0) {
                revert InvalidValidatorPower(i);
            }
            address currentAddress = _validatorAddress(i, validators[i]);
            if (i == 0) {
                continue;
            }

            ICometBFTMsgs.Validator memory prev = validators[i - 1];
            ICometBFTMsgs.Validator memory current = validators[i];
            address prevAddress = _validatorAddress(i - 1, prev);
            bool sorted = prev.votingPower > current.votingPower
                || (prev.votingPower == current.votingPower && uint160(prevAddress) < uint160(currentAddress));
            if (!sorted) {
                revert InvalidValidatorOrdering(i);
            }
        }
    }

    function _validatorAddress(uint256 index, ICometBFTMsgs.Validator memory validator) private pure returns (address) {
        (uint8 prefix, uint256 x) = _compressedPubKey(index, validator.pubKey);
        uint256 y = uint256(validator.y);

        if (y >= SECP256K1_P) {
            revert InvalidValidatorPubKeyWitness(index);
        }
        uint256 y2 = mulmod(y, y, SECP256K1_P);
        uint256 x2 = mulmod(x, x, SECP256K1_P);
        uint256 x3 = mulmod(x2, x, SECP256K1_P);
        if (y2 != addmod(x3, 7, SECP256K1_P)) {
            revert InvalidValidatorPubKeyWitness(index);
        }
        if (uint8(y & 1) != prefix - 2) {
            revert InvalidValidatorPubKeyWitness(index);
        }

        return address(uint160(uint256(keccak256(abi.encodePacked(x, y)))));
    }

    function _compressedPubKey(uint256 index, bytes memory pubKey) private pure returns (uint8 prefix, uint256 x) {
        if (pubKey.length != 33) {
            revert InvalidValidatorPubKey(index);
        }
        prefix = uint8(pubKey[0]);
        if (prefix != 0x02 && prefix != 0x03) {
            revert InvalidValidatorPubKey(index);
        }
        assembly ("memory-safe") {
            x := mload(add(pubKey, 33))
        }
        if (x >= SECP256K1_P) {
            revert InvalidValidatorPubKey(index);
        }
    }

    function _checkUpdateResult(
        ICometBFTMsgs.ConsensusState memory trustedConsensusState,
        uint64 newHeight,
        ICometBFTMsgs.ConsensusState memory newConsensusState
    )
        private
        view
        returns (ILightClientMsgs.UpdateResult)
    {
        bytes32 existingHash = consensusStateHashes[newHeight];
        bytes32 newHash = _consensusStateHash(newConsensusState);
        if (existingHash == bytes32(0)) {
            return ILightClientMsgs.UpdateResult.Update;
        }
        if (existingHash != newHash || trustedConsensusState.timestamp >= newConsensusState.timestamp) {
            return ILightClientMsgs.UpdateResult.Misbehaviour;
        }
        return ILightClientMsgs.UpdateResult.NoOp;
    }

    function _timestampNanos(uint64 seconds_, uint32 nanos) private pure returns (uint128) {
        return uint128(seconds_) * 1e9 + uint128(nanos);
    }

    function _consensusStateHash(ICometBFTMsgs.ConsensusState memory consensusState) private pure returns (bytes32) {
        return keccak256(abi.encode(consensusState));
    }

    modifier notFrozen() {
        if (clientState.isFrozen) {
            revert FrozenClientState();
        }
        _;
    }

    modifier onlyProofSubmitter() {
        if (!hasRole(PROOF_SUBMITTER_ROLE, address(0)) && !hasRole(PROOF_SUBMITTER_ROLE, msg.sender)) {
            _checkRole(PROOF_SUBMITTER_ROLE);
        }
        _;
    }
}
