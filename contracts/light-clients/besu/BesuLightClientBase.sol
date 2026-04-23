// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable gas-struct-packing, named-parameters-mapping, gas-strict-inequalities, code-complexity

import { AccessControl } from "@openzeppelin-contracts/access/AccessControl.sol";
import { ECDSA } from "@openzeppelin-contracts/utils/cryptography/ECDSA.sol";

import { ILightClient } from "../../interfaces/ILightClient.sol";
import { ILightClientMsgs } from "../../msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../msgs/IICS02ClientMsgs.sol";
import { IBesuLightClientMsgs } from "./msgs/IBesuLightClientMsgs.sol";
import { IBesuLightClientErrors } from "./errors/IBesuLightClientErrors.sol";
import { RLPReader } from "./RLPReader.sol";
import { MPTProof } from "./MPTProof.sol";

abstract contract BesuLightClientBase is ILightClient, IBesuLightClientErrors, IBesuLightClientMsgs, AccessControl {
    using MPTProof for bytes;
    using RLPReader for RLPReader.RLPItem;
    using RLPReader for bytes;

    struct ParsedHeader {
        bytes headerRlp;
        RLPReader.RLPItem[] headerItems;
        RLPReader.RLPItem[] extraDataItems;
        uint64 height;
        bytes32 stateRoot;
        uint64 timestamp;
        address[] validators;
        bytes[] commitSeals;
    }

    bytes32 public constant PROOF_SUBMITTER_ROLE = keccak256("PROOF_SUBMITTER_ROLE");

    bytes32 internal constant BESU_BFT_MIX_HASH = 0x63746963616c2062797a616e74696e65206661756c7420746f6c6572616e6365;
    bytes32 internal constant EMPTY_OMMERS_HASH = 0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347;
    bytes32 internal constant IBCSTORE_STORAGE_SLOT =
        0x1260944489272988d9df285149b5aa1b0f48f2136d6f416159f840a3e0747600;

    ClientState internal clientState;
    mapping(uint64 revisionHeight => ConsensusState) internal consensusStates;

    constructor(
        address ibcRouter,
        uint64 initialTrustedHeight,
        uint64 initialTrustedTimestamp,
        bytes32 initialTrustedStorageRoot,
        address[] memory initialTrustedValidators,
        uint64 trustingPeriod,
        uint64 maxClockDrift,
        address roleManager
    ) {
        if (initialTrustedHeight == 0) {
            revert InvalidHeaderHeight();
        }

        _validateValidators(initialTrustedValidators);

        clientState = ClientState({
            ibcRouter: ibcRouter,
            latestHeight: IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: initialTrustedHeight }),
            trustingPeriod: trustingPeriod,
            maxClockDrift: maxClockDrift
        });

        ConsensusState storage consensusState = consensusStates[initialTrustedHeight];
        consensusState.timestamp = initialTrustedTimestamp;
        consensusState.storageRoot = initialTrustedStorageRoot;
        consensusState.validators = initialTrustedValidators;

        if (roleManager == address(0)) {
            _grantRole(PROOF_SUBMITTER_ROLE, address(0));
        } else {
            _grantRole(DEFAULT_ADMIN_ROLE, roleManager);
            _grantRole(PROOF_SUBMITTER_ROLE, roleManager);
        }
    }

    function getClientState() external view returns (bytes memory) {
        return abi.encode(clientState);
    }

    function getConsensusState(uint64 revisionHeight) external view returns (bytes memory) {
        ConsensusState storage consensusState = _getConsensusState(revisionHeight);
        return abi.encode(consensusState.timestamp, consensusState.storageRoot, consensusState.validators);
    }

    function updateClient(bytes calldata updateMsg)
        external
        onlyProofSubmitter
        returns (ILightClientMsgs.UpdateResult)
    {
        MsgUpdateClient memory msg_ = abi.decode(updateMsg, (MsgUpdateClient));
        _requireZeroRevision(msg_.trustedHeight.revisionNumber);

        ParsedHeader memory header = _parseHeader(msg_.headerRlp);
        if (header.height == 0) {
            revert InvalidHeaderHeight();
        }
        if (block.timestamp + clientState.maxClockDrift < header.timestamp) {
            revert HeaderFromFuture(block.timestamp, header.timestamp, clientState.maxClockDrift);
        }

        ConsensusState storage trustedConsensusState = _getConsensusState(msg_.trustedHeight.revisionHeight);
        if (
            clientState.trustingPeriod != 0
                && uint256(trustedConsensusState.timestamp) + clientState.trustingPeriod <= block.timestamp
        ) {
            revert ConsensusStateExpired(trustedConsensusState.timestamp, block.timestamp, clientState.trustingPeriod);
        }

        address[] memory signers = _recoverSigners(_commitSealDigest(header), header.commitSeals);
        _checkTrustedValidatorOverlap(signers, trustedConsensusState.validators);
        _checkValidatorQuorum(signers, header.validators);

        bytes32 storageRoot = _verifyAccountProof(clientState.ibcRouter, header.stateRoot, msg_.accountProof);

        ConsensusState storage existingConsensusState = consensusStates[header.height];
        if (existingConsensusState.timestamp != 0) {
            if (_isSameConsensusState(existingConsensusState, header.timestamp, storageRoot, header.validators)) {
                return ILightClientMsgs.UpdateResult.NoOp;
            }
            revert ConflictingConsensusState(header.height);
        }

        ConsensusState storage newConsensusState = consensusStates[header.height];
        newConsensusState.timestamp = header.timestamp;
        newConsensusState.storageRoot = storageRoot;
        newConsensusState.validators = header.validators;

        if (header.height > clientState.latestHeight.revisionHeight) {
            clientState.latestHeight.revisionHeight = header.height;
        }

        return ILightClientMsgs.UpdateResult.Update;
    }

    function verifyMembership(ILightClientMsgs.MsgVerifyMembership calldata msg_)
        external
        view
        onlyProofSubmitter
        returns (uint256)
    {
        _requireZeroRevision(msg_.proofHeight.revisionNumber);
        if (msg_.path.length != 1) {
            revert InvalidPathLength(1, msg_.path.length);
        }
        if (msg_.value.length != 32) {
            revert InvalidValueLength(32, msg_.value.length);
        }

        ConsensusState storage consensusState = _getConsensusState(msg_.proofHeight.revisionHeight);
        bytes32 storageSlot = _commitmentStorageSlot(msg_.path[0]);
        bytes32 expectedValue = abi.decode(msg_.value, (bytes32));
        bytes memory valueRlp =
            msg_.proof.verifyRLPProof(consensusState.storageRoot, keccak256(abi.encodePacked(storageSlot)));
        if (valueRlp.length == 0) {
            revert InvalidCommitmentValue(expectedValue, bytes32(0));
        }

        bytes32 actualValue = bytes32(valueRlp.toRlpItem().toUint());
        if (actualValue != expectedValue) {
            revert InvalidCommitmentValue(expectedValue, actualValue);
        }
        return consensusState.timestamp;
    }

    function verifyNonMembership(ILightClientMsgs.MsgVerifyNonMembership calldata msg_)
        external
        view
        onlyProofSubmitter
        returns (uint256)
    {
        _requireZeroRevision(msg_.proofHeight.revisionNumber);
        if (msg_.path.length != 1) {
            revert InvalidPathLength(1, msg_.path.length);
        }

        ConsensusState storage consensusState = _getConsensusState(msg_.proofHeight.revisionHeight);
        bytes32 storageSlot = _commitmentStorageSlot(msg_.path[0]);
        bytes memory valueRlp =
            msg_.proof.verifyRLPProof(consensusState.storageRoot, keccak256(abi.encodePacked(storageSlot)));
        if (valueRlp.length != 0) {
            revert ValueExists(bytes32(valueRlp.toRlpItem().toUint()));
        }
        return consensusState.timestamp;
    }

    function misbehaviour(bytes calldata) external view onlyProofSubmitter {
        revert UnsupportedMisbehaviour();
    }

    function _commitSealDigest(ParsedHeader memory header) internal pure virtual returns (bytes32);

    function _parseHeader(bytes memory headerRlp) internal pure returns (ParsedHeader memory header) {
        header.headerRlp = headerRlp;
        header.headerItems = headerRlp.toRlpItem().toList();
        if (header.headerItems.length < 15) {
            revert InvalidHeaderFormat(header.headerItems.length);
        }

        if (bytes32(header.headerItems[1].toUintStrict()) != EMPTY_OMMERS_HASH) {
            revert InvalidOmmersHash(bytes32(header.headerItems[1].toUintStrict()));
        }
        if (header.headerItems[7].toUint() != 1) {
            revert InvalidDifficulty(header.headerItems[7].toUint());
        }
        if (bytes32(header.headerItems[13].toUintStrict()) != BESU_BFT_MIX_HASH) {
            revert InvalidMixHash(bytes32(header.headerItems[13].toUintStrict()));
        }

        bytes memory nonce = header.headerItems[14].toBytes();
        if (nonce.length != 8 || keccak256(nonce) != keccak256(hex"0000000000000000")) {
            revert InvalidNonce(nonce);
        }

        header.height = uint64(header.headerItems[8].toUint());
        header.stateRoot = bytes32(header.headerItems[3].toUintStrict());
        header.timestamp = uint64(header.headerItems[11].toUint());

        bytes memory extraData = header.headerItems[12].toBytes();
        header.extraDataItems = extraData.toRlpItem().toList();
        if (header.extraDataItems.length != 5) {
            revert InvalidExtraDataFormat(header.extraDataItems.length);
        }

        RLPReader.RLPItem[] memory validatorItems = header.extraDataItems[1].toList();
        if (validatorItems.length == 0) {
            revert EmptyValidatorSet();
        }

        header.validators = new address[](validatorItems.length);
        for (uint256 i = 0; i < validatorItems.length; ++i) {
            bytes memory validatorBytes = validatorItems[i].toBytes();
            if (validatorBytes.length != 20) {
                revert InvalidValidatorAddressLength(validatorBytes.length);
            }

            address validator = address(bytes20(validatorBytes));
            if (validator == address(0)) {
                revert InvalidValidatorAddress(validator);
            }
            for (uint256 j = 0; j < i; ++j) {
                if (header.validators[j] == validator) {
                    revert DuplicateValidator(validator);
                }
            }
            header.validators[i] = validator;
        }

        RLPReader.RLPItem[] memory sealItems = header.extraDataItems[4].toList();
        header.commitSeals = new bytes[](sealItems.length);
        for (uint256 i = 0; i < sealItems.length; ++i) {
            header.commitSeals[i] = sealItems[i].toBytes();
        }
    }

    function _verifyAccountProof(
        address account,
        bytes32 stateRoot,
        bytes memory accountProof
    )
        internal
        pure
        returns (bytes32)
    {
        bytes memory accountRlp = accountProof.verifyRLPProof(stateRoot, keccak256(abi.encodePacked(account)));
        RLPReader.RLPItem[] memory accountItems = accountRlp.toRlpItem().toList();
        return bytes32(accountItems[2].toUintStrict());
    }

    function _commitmentStorageSlot(bytes memory rawPath) internal pure returns (bytes32) {
        return keccak256(abi.encode(keccak256(rawPath), IBCSTORE_STORAGE_SLOT));
    }

    function _recoverSigners(bytes32 digest, bytes[] memory seals) internal pure returns (address[] memory signers) {
        signers = new address[](seals.length);
        for (uint256 i = 0; i < seals.length; ++i) {
            address signer = _recoverSigner(digest, seals[i]);
            for (uint256 j = 0; j < i; ++j) {
                if (signers[j] == signer) {
                    revert DuplicateCommitSealSigner(signer);
                }
            }
            signers[i] = signer;
        }
    }

    function _recoverSigner(bytes32 digest, bytes memory seal) internal pure returns (address) {
        if (seal.length != 65) {
            revert InvalidECDSASignatureLength(seal.length);
        }
        if (uint8(seal[64]) < 27) {
            seal[64] = bytes1(uint8(seal[64]) + 27);
        }
        (address signer, ECDSA.RecoverError err,) = ECDSA.tryRecover(digest, seal);
        if (err != ECDSA.RecoverError.NoError || signer == address(0)) {
            revert InvalidCommitSeal();
        }
        return signer;
    }

    function _checkTrustedValidatorOverlap(
        address[] memory signers,
        address[] storage trustedValidators
    )
        internal
        view
    {
        uint256 actual;
        for (uint256 i = 0; i < signers.length; ++i) {
            if (_containsStorage(trustedValidators, signers[i])) {
                ++actual;
            }
        }

        uint256 required = trustedValidators.length / 3 + 1;
        if (actual < required) {
            revert InsufficientTrustedValidatorOverlap(actual, required);
        }
    }

    function _checkValidatorQuorum(address[] memory signers, address[] memory validators) internal pure {
        uint256 actual;
        for (uint256 i = 0; i < signers.length; ++i) {
            if (_containsMemory(validators, signers[i])) {
                ++actual;
            }
        }

        uint256 required = validators.length * 2 / 3 + 1;
        if (actual < required) {
            revert InsufficientValidatorQuorum(actual, required);
        }
    }

    function _validateValidators(address[] memory validators) internal pure {
        if (validators.length == 0) {
            revert EmptyValidatorSet();
        }
        for (uint256 i = 0; i < validators.length; ++i) {
            if (validators[i] == address(0)) {
                revert InvalidValidatorAddress(validators[i]);
            }
            for (uint256 j = 0; j < i; ++j) {
                if (validators[j] == validators[i]) {
                    revert DuplicateValidator(validators[i]);
                }
            }
        }
    }

    function _isSameConsensusState(
        ConsensusState storage consensusState,
        uint64 timestamp,
        bytes32 storageRoot,
        address[] memory validators
    )
        internal
        view
        returns (bool)
    {
        if (consensusState.timestamp != timestamp || consensusState.storageRoot != storageRoot) {
            return false;
        }
        if (consensusState.validators.length != validators.length) {
            return false;
        }
        for (uint256 i = 0; i < validators.length; ++i) {
            if (consensusState.validators[i] != validators[i]) {
                return false;
            }
        }
        return true;
    }

    function _getConsensusState(uint64 revisionHeight) internal view returns (ConsensusState storage consensusState) {
        consensusState = consensusStates[revisionHeight];
        if (consensusState.timestamp == 0) {
            revert ConsensusStateNotFound(revisionHeight);
        }
    }

    function _requireZeroRevision(uint64 revisionNumber) internal pure {
        if (revisionNumber != 0) {
            revert InvalidRevisionNumber(revisionNumber);
        }
    }

    function _containsStorage(address[] storage validators, address signer) internal view returns (bool) {
        for (uint256 i = 0; i < validators.length; ++i) {
            if (validators[i] == signer) {
                return true;
            }
        }
        return false;
    }

    function _containsMemory(address[] memory validators, address signer) internal pure returns (bool) {
        for (uint256 i = 0; i < validators.length; ++i) {
            if (validators[i] == signer) {
                return true;
            }
        }
        return false;
    }

    function _rlpItemBytes(RLPReader.RLPItem memory item) internal pure returns (bytes memory) {
        return item.toRlpBytes();
    }

    function _encodeRlpBytes(bytes memory raw) internal pure returns (bytes memory) {
        if (raw.length == 1 && uint8(raw[0]) < 0x80) {
            return raw;
        }
        if (raw.length < 56) {
            return abi.encodePacked(bytes1(uint8(0x80 + raw.length)), raw);
        }

        bytes memory lenBytes = _encodeLength(raw.length);
        return abi.encodePacked(bytes1(uint8(0xb7 + lenBytes.length)), lenBytes, raw);
    }

    function _encodeRlpList(bytes[] memory items) internal pure returns (bytes memory) {
        bytes memory payload;
        for (uint256 i = 0; i < items.length; ++i) {
            payload = bytes.concat(payload, items[i]);
        }

        if (payload.length < 56) {
            return abi.encodePacked(bytes1(uint8(0xc0 + payload.length)), payload);
        }

        bytes memory lenBytes = _encodeLength(payload.length);
        return abi.encodePacked(bytes1(uint8(0xf7 + lenBytes.length)), lenBytes, payload);
    }

    function _encodeLength(uint256 value) internal pure returns (bytes memory) {
        uint256 tmp = value;
        uint256 length;
        while (tmp != 0) {
            ++length;
            tmp >>= 8;
        }

        bytes memory out = new bytes(length);
        tmp = value;
        for (uint256 i = length; i > 0; --i) {
            out[i - 1] = bytes1(uint8(tmp));
            tmp >>= 8;
        }
        return out;
    }

    modifier onlyProofSubmitter() {
        if (!hasRole(PROOF_SUBMITTER_ROLE, address(0))) {
            _checkRole(PROOF_SUBMITTER_ROLE);
        }
        _;
    }
}
