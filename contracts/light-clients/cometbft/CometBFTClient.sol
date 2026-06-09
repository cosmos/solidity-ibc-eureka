// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { AccessControl } from "@openzeppelin-contracts/access/AccessControl.sol";

import { ILightClient } from "../../interfaces/ILightClient.sol";
import { ILightClientMsgs } from "../../msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../msgs/IICS02ClientMsgs.sol";
import { ICometBFTClientErrors } from "./errors/ICometBFTClientErrors.sol";
import { ICometBFTMsgs } from "./msgs/ICometBFTMsgs.sol";
import { CometBFTECDSA } from "./utils/CometBFTECDSA.sol";
import { CometBFTICS23 } from "./utils/CometBFTICS23.sol";
import { CometBFTProto } from "./utils/CometBFTProto.sol";

/// @title Native CometBFT Light Client
/// @notice Native adjacent-update CometBFT light client for secp256k1eth validator sets.
contract CometBFTClient is ILightClient, ICometBFTClientErrors, AccessControl {
    bytes32 public constant PROOF_SUBMITTER_ROLE = keccak256("PROOF_SUBMITTER_ROLE");
    uint8 private constant BLOCK_ID_FLAG_ABSENT = 1;
    uint8 private constant BLOCK_ID_FLAG_COMMIT = 2;
    uint8 private constant BLOCK_ID_FLAG_NIL = 3;
    uint256 private constant SECP256K1_P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F;
    uint32 private constant NANOS_PER_SECOND = 1_000_000_000;

    ICometBFTMsgs.ClientState private clientState;
    mapping(bytes32 heightKey => bool exists) private consensusStateExists;
    mapping(bytes32 heightKey => ICometBFTMsgs.ConsensusState consensusState) private consensusStates;

    constructor(
        ICometBFTMsgs.ClientState memory initialClientState,
        ICometBFTMsgs.ConsensusState memory initialConsensusState,
        address roleManager
    ) {
        _validateInitialClientState(initialClientState);

        clientState = initialClientState;
        _storeConsensusState(initialClientState.latestHeight, initialConsensusState);

        _grantRole(DEFAULT_ADMIN_ROLE, roleManager);
        _grantRole(PROOF_SUBMITTER_ROLE, roleManager);
    }

    /// @inheritdoc ILightClient
    function getClientState() external view returns (bytes memory) {
        return abi.encode(clientState);
    }

    /// @notice Returns the stored consensus-state hash for a full IBC height.
    function getConsensusStateHash(IICS02ClientMsgs.Height memory height) public view returns (bytes32) {
        return _consensusStateHash(_getConsensusState(height));
    }

    /// @notice Returns the stored consensus-state hash for a revision height in the current client revision.
    function getConsensusStateHash(uint64 revisionHeight) public view returns (bytes32) {
        return getConsensusStateHash(
            IICS02ClientMsgs.Height({
                revisionNumber: clientState.latestHeight.revisionNumber, revisionHeight: revisionHeight
            })
        );
    }

    /// @notice Returns the stored consensus state for a full IBC height.
    function getConsensusState(IICS02ClientMsgs.Height memory height)
        public
        view
        returns (ICometBFTMsgs.ConsensusState memory)
    {
        return _getConsensusState(height);
    }

    /// @inheritdoc ILightClient
    function updateClient(bytes calldata updateMsg)
        external
        notFrozen
        onlyProofSubmitter
        returns (ILightClientMsgs.UpdateResult)
    {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = abi.decode(updateMsg, (ICometBFTMsgs.MsgUpdateClient));

        _validateTrustedConsensusState(msg_.trustedHeight, msg_.trustedConsensusState);
        _validateHeaderAndValidatorSet(msg_, true);
        _verifyCommit(msg_.header.chainId, msg_.commit, msg_.validators);

        ICometBFTMsgs.ConsensusState memory newConsensusState = ICometBFTMsgs.ConsensusState({
            timestamp: _timestampNanos(msg_.header.timeSeconds, msg_.header.timeNanos),
            root: msg_.header.appHash,
            nextValidatorsHash: msg_.header.nextValidatorsHash
        });

        ILightClientMsgs.UpdateResult result = _checkUpdateResult(msg_.header.height, newConsensusState);
        if (result == ILightClientMsgs.UpdateResult.Update) {
            _storeConsensusState(_headerIBCHeight(msg_.header.height), newConsensusState);
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
    function verifyMembership(ILightClientMsgs.MsgVerifyMembership calldata msg_)
        external
        view
        notFrozen
        onlyProofSubmitter
        returns (uint256)
    {
        ICometBFTMsgs.ConsensusState storage consensusState = _getConsensusState(msg_.proofHeight);
        CometBFTICS23.verifyMembership(consensusState.root, msg_.proof, msg_.path, msg_.value);
        return uint256(consensusState.timestamp / 1e9);
    }

    /// @inheritdoc ILightClient
    function verifyNonMembership(ILightClientMsgs.MsgVerifyNonMembership calldata msg_)
        external
        view
        notFrozen
        onlyProofSubmitter
        returns (uint256)
    {
        ICometBFTMsgs.ConsensusState storage consensusState = _getConsensusState(msg_.proofHeight);
        CometBFTICS23.verifyNonMembership(consensusState.root, msg_.proof, msg_.path);
        return uint256(consensusState.timestamp / 1e9);
    }

    /// @inheritdoc ILightClient
    function misbehaviour(bytes calldata misbehaviourMsg) external notFrozen onlyProofSubmitter {
        ICometBFTMsgs.MsgSubmitMisbehaviour memory msg_ =
            abi.decode(misbehaviourMsg, (ICometBFTMsgs.MsgSubmitMisbehaviour));

        _validateMisbehaviourUpdate(msg_.updateA);
        _validateMisbehaviourUpdate(msg_.updateB);
        if (!_isMisbehaviour(msg_.updateA.header, msg_.updateB.header)) {
            revert InvalidMisbehaviour();
        }

        clientState.isFrozen = true;
    }

    function _validateTrustedConsensusState(
        IICS02ClientMsgs.Height memory trustedHeight,
        ICometBFTMsgs.ConsensusState memory trustedConsensusState
    )
        private
        view
    {
        if (trustedHeight.revisionNumber != clientState.latestHeight.revisionNumber) {
            revert RevisionNumberMismatch(clientState.latestHeight.revisionNumber, trustedHeight.revisionNumber);
        }
        bytes32 expected = getConsensusStateHash(trustedHeight);
        bytes32 actual = _consensusStateHash(trustedConsensusState);
        if (actual != expected) {
            revert ConsensusStateHashMismatch(expected, actual);
        }
    }

    function _validateHeaderAndValidatorSet(
        ICometBFTMsgs.MsgUpdateClient memory msg_,
        bool requireTimeIncreasing
    )
        private
        view
    {
        ICometBFTMsgs.Header memory header_ = msg_.header;
        ICometBFTMsgs.ConsensusState memory trustedConsensusState = msg_.trustedConsensusState;

        if (keccak256(bytes(header_.chainId)) != keccak256(bytes(clientState.chainId))) {
            revert ChainIdMismatch(clientState.chainId, header_.chainId);
        }
        if (header_.height != msg_.trustedHeight.revisionHeight + 1) {
            revert UnsupportedNonAdjacentUpdate(msg_.trustedHeight.revisionHeight, header_.height);
        }

        if (header_.timeNanos >= NANOS_PER_SECOND) {
            revert InvalidHeaderTimestampNanos(header_.timeNanos);
        }
        uint128 headerTime = _timestampNanos(header_.timeSeconds, header_.timeNanos);
        if (requireTimeIncreasing && headerTime <= trustedConsensusState.timestamp) {
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

    function _validateMisbehaviourUpdate(ICometBFTMsgs.MsgUpdateClient memory msg_) private view {
        _validateTrustedConsensusState(msg_.trustedHeight, msg_.trustedConsensusState);
        _validateHeaderAndValidatorSet(msg_, false);
        _verifyCommit(msg_.header.chainId, msg_.commit, msg_.validators);
    }

    function _isMisbehaviour(
        ICometBFTMsgs.Header memory headerA,
        ICometBFTMsgs.Header memory headerB
    )
        private
        pure
        returns (bool)
    {
        if (headerA.height == headerB.height) {
            return CometBFTProto.headerHash(headerA) != CometBFTProto.headerHash(headerB);
        }

        uint128 timeA = _timestampNanos(headerA.timeSeconds, headerA.timeNanos);
        uint128 timeB = _timestampNanos(headerB.timeSeconds, headerB.timeNanos);
        if (headerA.height < headerB.height) {
            return timeB <= timeA;
        }
        return timeA <= timeB;
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
        if (sig.timestampNanos >= NANOS_PER_SECOND) {
            revert InvalidCommitTimestampNanos(index, sig.timestampNanos);
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
        uint64 newHeight,
        ICometBFTMsgs.ConsensusState memory newConsensusState
    )
        private
        view
        returns (ILightClientMsgs.UpdateResult)
    {
        IICS02ClientMsgs.Height memory newIBCHeight = _headerIBCHeight(newHeight);
        bytes32 heightKey = _heightKey(newIBCHeight);
        bytes32 newHash = _consensusStateHash(newConsensusState);
        if (!consensusStateExists[heightKey]) {
            return ILightClientMsgs.UpdateResult.Update;
        }
        bytes32 existingHash = _consensusStateHash(consensusStates[heightKey]);
        if (existingHash != newHash) {
            return ILightClientMsgs.UpdateResult.Misbehaviour;
        }
        return ILightClientMsgs.UpdateResult.NoOp;
    }

    function _timestampNanos(uint64 seconds_, uint32 nanos) private pure returns (uint128) {
        return uint128(seconds_) * NANOS_PER_SECOND + uint128(nanos);
    }

    function _validateInitialClientState(ICometBFTMsgs.ClientState memory initialClientState) private pure {
        if (
            bytes(initialClientState.chainId).length == 0 || initialClientState.latestHeight.revisionHeight == 0
                || initialClientState.trustLevel.denominator == 0 || initialClientState.trustLevel.numerator == 0
                || initialClientState.trustLevel.numerator > initialClientState.trustLevel.denominator
                || initialClientState.trustLevel.numerator * 3 < initialClientState.trustLevel.denominator
                || initialClientState.trustingPeriod == 0 || initialClientState.unbondingPeriod == 0
                || initialClientState.trustingPeriod >= initialClientState.unbondingPeriod
                || initialClientState.maxClockDrift == 0
        ) {
            revert InvalidClientState();
        }
    }

    function _headerIBCHeight(uint64 revisionHeight) private view returns (IICS02ClientMsgs.Height memory) {
        return IICS02ClientMsgs.Height({
            revisionNumber: clientState.latestHeight.revisionNumber, revisionHeight: revisionHeight
        });
    }

    function _heightKey(IICS02ClientMsgs.Height memory height) private pure returns (bytes32) {
        return keccak256(abi.encode(height.revisionNumber, height.revisionHeight));
    }

    function _getConsensusState(IICS02ClientMsgs.Height memory height)
        private
        view
        returns (ICometBFTMsgs.ConsensusState storage)
    {
        bytes32 key = _heightKey(height);
        if (!consensusStateExists[key]) {
            revert ConsensusStateNotFound(height.revisionNumber, height.revisionHeight);
        }
        return consensusStates[key];
    }

    function _storeConsensusState(
        IICS02ClientMsgs.Height memory height,
        ICometBFTMsgs.ConsensusState memory consensusState
    )
        private
    {
        bytes32 key = _heightKey(height);
        consensusStates[key] = consensusState;
        consensusStateExists[key] = true;
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
