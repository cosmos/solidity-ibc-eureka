// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { AccessControl } from "@openzeppelin-contracts/access/AccessControl.sol";
import { ECDSA } from "@openzeppelin-contracts/utils/cryptography/ECDSA.sol";

import { ILightClient } from "../interfaces/ILightClient.sol";
import { ILightClientMsgs } from "../msgs/ILightClientMsgs.sol";
import { IAttestorLightClient } from "./IAttestorLightClient.sol";
import { IAttestorLightClientErrors } from "./errors/IAttestorLightClientErrors.sol";
import { IAttestorMsgs } from "./msgs/IAttestorMsgs.sol";
import { IAttestorLightClientMsgs } from "./msgs/IAttestorLightClientMsgs.sol";

/// @title Attestor-based IBC Light Client
/// @notice Implements an IBC light client that trusts an off-chain m-of-n attestor set.
contract AttestorLightClient is IAttestorLightClient, IAttestorLightClientErrors, AccessControl {
    /// @notice Current attestor-set configuration and latest trusted height/frozen flag.
    IAttestorLightClientMsgs.ClientState private clientState;
    /// @notice Tracks whether an `attestor` address is part of the configured attestor set.
    mapping(address attestor => bool isAttestor) private _isAttestor;
    /// @notice Trusted consensus timestamp in seconds for a given `height`.
    mapping(uint64 height => uint64 timestampSeconds) private _consensusTimestampAtHeight;

    /// @inheritdoc IAttestorLightClient
    bytes32 public constant PROOF_SUBMITTER_ROLE = keccak256("PROOF_SUBMITTER_ROLE");

    /// @notice Initializes the attestor light client with its fixed attestor set and initial height/timestamp.
    /// @param attestorAddresses The configured attestor addresses (EOAs)
    /// @param minRequiredSigs The quorum threshold
    /// @param initialHeight The initial known height
    /// @param initialTimestampSeconds The initial timestamp in seconds for the initial height
    /// @param roleManager Address that will administer roles and be allowed to submit proofs; if zero, anyone can
    /// submit
    constructor(
        address[] memory attestorAddresses,
        uint8 minRequiredSigs,
        uint64 initialHeight,
        uint64 initialTimestampSeconds,
        address roleManager
    ) {
        if (attestorAddresses.length == 0) revert NoAttestors();
        if (minRequiredSigs == 0 || minRequiredSigs > attestorAddresses.length) {
            revert BadQuorum(minRequiredSigs, attestorAddresses.length);
        }

        clientState = IAttestorLightClientMsgs.ClientState({
            attestorAddresses: attestorAddresses,
            minRequiredSigs: minRequiredSigs,
            latestHeight: initialHeight,
            isFrozen: false
        });

        for (uint256 i = 0; i < attestorAddresses.length; i++) {
            _isAttestor[attestorAddresses[i]] = true;
        }

        _consensusTimestampAtHeight[initialHeight] = initialTimestampSeconds;

        if (roleManager == address(0)) {
            _grantRole(PROOF_SUBMITTER_ROLE, address(0)); // allow anyone
        } else {
            _grantRole(DEFAULT_ADMIN_ROLE, roleManager);
            _grantRole(PROOF_SUBMITTER_ROLE, roleManager);
        }
    }

    /// @inheritdoc ILightClient
    function getClientState() external view returns (bytes memory) {
        return abi.encode(clientState);
    }

    /// @inheritdoc IAttestorLightClient
    function getAttestorSet() external view returns (address[] memory attestorAddresses, uint8 minRequiredSigs) {
        return (clientState.attestorAddresses, clientState.minRequiredSigs);
    }

    /// @inheritdoc IAttestorLightClient
    function getConsensusTimestamp(uint64 revisionHeight) external view returns (uint64) {
        return _consensusTimestampAtHeight[revisionHeight];
    }

    /// @inheritdoc ILightClient
    function updateClient(bytes calldata updateMsg)
        external
        notFrozen
        onlyProofSubmitter
        returns (ILightClientMsgs.UpdateResult)
    {
        IAttestorMsgs.AttestationProof memory proof = abi.decode(updateMsg, (IAttestorMsgs.AttestationProof));

        bytes32 digest = sha256(proof.attestationData);
        _verifySignatures(digest, proof.signatures);

        IAttestorMsgs.StateAttestation memory state =
            abi.decode(proof.attestationData, (IAttestorMsgs.StateAttestation));

        // Check if height already exists, if it does, check if the timestamp is the same, otherwise revert
        if (_consensusTimestampAtHeight[state.height] != 0) {
            if (_consensusTimestampAtHeight[state.height] != state.timestamp) {
                revert ConflictingTimestamp(state.height, _consensusTimestampAtHeight[state.height], state.timestamp);
            }
            return ILightClientMsgs.UpdateResult.NoOp;
        }

        _consensusTimestampAtHeight[state.height] = state.timestamp;
        clientState.latestHeight = state.height;
        return ILightClientMsgs.UpdateResult.Update;
    }

    /// @inheritdoc ILightClient
    function verifyMembership(ILightClientMsgs.MsgVerifyMembership calldata msg_)
        external
        view
        notFrozen
        onlyProofSubmitter
        returns (uint256)
    {
        if (msg_.value.length == 0) revert EmptyValue();

        // Ensure we have a trusted timestamp at the provided height.
        uint64 height = msg_.proofHeight.revisionHeight;
        uint64 ts = _consensusTimestampAtHeight[height];
        if (ts == 0) revert ConsensusTimestampNotFound(height);

        IAttestorMsgs.AttestationProof memory proof = abi.decode(msg_.proof, (IAttestorMsgs.AttestationProof));
        bytes32 digest = sha256(proof.attestationData);
        _verifySignatures(digest, proof.signatures);

        // Decode the attested packet commitments and verify the attested height matches the provided proof height
        IAttestorMsgs.PacketAttestation memory packetAttestation =
            abi.decode(proof.attestationData, (IAttestorMsgs.PacketAttestation));

        if (packetAttestation.height != height) revert HeightMismatch(height, packetAttestation.height);

        // Check membership: value must be present in the attested list
        bool found = false;
        bytes32 value = abi.decode(msg_.value, (bytes32));
        for (uint256 i = 0; i < packetAttestation.packets.length; i++) {
            if (packetAttestation.packets[i] == value) {
                found = true;
                break;
            }
        }
        if (!found) revert NotMember();

        return uint256(ts);
    }

    /// @inheritdoc ILightClient
    function verifyNonMembership(ILightClientMsgs.MsgVerifyNonMembership calldata)
        external
        view
        notFrozen
        onlyProofSubmitter
        returns (uint256)
    {
        // Out of scope for this version
        revert FeatureNotSupported();
    }

    /// @inheritdoc ILightClient
    function misbehaviour(bytes calldata) external view notFrozen onlyProofSubmitter {
        // Out of scope for this version
        revert FeatureNotSupported();
    }

    /// @inheritdoc ILightClient
    function upgradeClient(bytes calldata) external view notFrozen onlyProofSubmitter {
        revert FeatureNotSupported();
    }

    /// @notice Verifies that `signatures` over `digest` are valid, unique, and meet the threshold.
    /// @param digest The message hash that attestors must have signed.
    /// @param signatures Compact ECDSA signatures (r||s||v) provided by attestors.
    /// @dev Reverts with `InvalidSignatureLength`, `SignatureInvalid`, `UnknownSigner`, `DuplicateSigner`,
    ///      or `ThresholdNotMet` on failure.
    function _verifySignatures(bytes32 digest, bytes[] memory signatures) private view {
        address[] memory seen = new address[](signatures.length);
        uint256 seenLen = 0;

        uint256 valid = 0;
        for (uint256 i = 0; i < signatures.length; i++) {
            bytes memory sig = signatures[i];
            if (sig.length != 65) revert InvalidSignatureLength(i);

            address recovered = ECDSA.recover(digest, sig);
            if (recovered == address(0)) revert SignatureInvalid(i);

            if (!_isAttestor[recovered]) revert UnknownSigner(recovered);

            // check duplicates
            for (uint256 j = 0; j < seenLen; j++) {
                if (seen[j] == recovered) revert DuplicateSigner(recovered);
            }
            seen[seenLen++] = recovered;
            valid++;
        }

        if (valid < clientState.minRequiredSigs) revert ThresholdNotMet(valid, clientState.minRequiredSigs);
    }

    /// @notice Reverts if the client state is frozen.
    modifier notFrozen() {
        if (clientState.isFrozen) revert FrozenClientState();
        _;
    }

    /// @notice Restricts access to addresses with `PROOF_SUBMITTER_ROLE` unless the role is open to anyone.
    modifier onlyProofSubmitter() {
        if (!hasRole(PROOF_SUBMITTER_ROLE, address(0))) {
            _checkRole(PROOF_SUBMITTER_ROLE);
        }
        _;
    }
}
