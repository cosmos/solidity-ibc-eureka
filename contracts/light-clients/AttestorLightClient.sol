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
        require(attestorAddresses.length > 0, NoAttestors());
        require(
            minRequiredSigs > 0 && attestorAddresses.length > minRequiredSigs - 1,
            BadQuorum(minRequiredSigs, attestorAddresses.length)
        );

        clientState = IAttestorLightClientMsgs.ClientState({
            attestorAddresses: attestorAddresses,
            minRequiredSigs: minRequiredSigs,
            latestHeight: initialHeight,
            isFrozen: false
        });

        for (uint256 i = 0; i < attestorAddresses.length; ++i) {
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
        _verifySignaturesThreshold(digest, proof.signatures);

        IAttestorMsgs.StateAttestation memory state =
            abi.decode(proof.attestationData, (IAttestorMsgs.StateAttestation));

        require(state.height > 0 && state.timestamp > 0, InvalidState(state.height, state.timestamp));

        // Check if height already exists, if it does, check if the timestamp is the same, otherwise freeze the client
        // and return UpdateResult.Misbehaviour
        if (_consensusTimestampAtHeight[state.height] != 0) {
            if (_consensusTimestampAtHeight[state.height] != state.timestamp) {
                clientState.isFrozen = true;
                return ILightClientMsgs.UpdateResult.Misbehaviour;
            }
            return ILightClientMsgs.UpdateResult.NoOp;
        }

        if (state.height > clientState.latestHeight) {
            clientState.latestHeight = state.height;
        }
        _consensusTimestampAtHeight[state.height] = state.timestamp;

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
        require(msg_.value.length != 0, EmptyValue());

        // Ensure we have a trusted timestamp at the provided height.
        uint64 proofHeight = msg_.proofHeight.revisionHeight;
        uint64 ts = _consensusTimestampAtHeight[proofHeight];
        require(ts != 0, ConsensusTimestampNotFound(proofHeight));

        IAttestorMsgs.AttestationProof memory proof = abi.decode(msg_.proof, (IAttestorMsgs.AttestationProof));
        bytes32 digest = sha256(proof.attestationData);
        _verifySignaturesThreshold(digest, proof.signatures);

        // Decode the attested packet commitments and verify the attested height matches the provided proof height
        IAttestorMsgs.PacketAttestation memory packetAttestation =
            abi.decode(proof.attestationData, (IAttestorMsgs.PacketAttestation));

        // Ensure the attested height matches the requested proofHeight
        require(packetAttestation.height == proofHeight, HeightMismatch(proofHeight, packetAttestation.height));

        require(packetAttestation.packets.length > 0, EmptyPacketCommitments());
        // Check membership: value must be present in the attested list
        bytes32 value = abi.decode(msg_.value, (bytes32));
        for (uint256 i = 0; i < packetAttestation.packets.length; ++i) {
            if (packetAttestation.packets[i].commitment == value) {
                return uint256(ts);
            }
        }

        revert NotMember();
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
    function _verifySignaturesThreshold(bytes32 digest, bytes[] memory signatures) private view {
        require(signatures.length > 0, EmptySignatures());

        address[] memory seen = new address[](signatures.length);

        for (uint256 i = 0; i < signatures.length; ++i) {
            bytes memory sig = signatures[i];
            address recovered = _verifySignature(digest, sig);

            // check duplicates
            for (uint256 j = 0; j < i; ++j) {
                require(seen[j] != recovered, DuplicateSigner(recovered));
            }
            seen[i] = recovered;
        }

        require(
            signatures.length > clientState.minRequiredSigs - 1,
            ThresholdNotMet(signatures.length, clientState.minRequiredSigs)
        );
    }

    /// @notice Verifies a single signature and returns the recovered signer address.
    /// @param digest The message hash that was signed.
    /// @param signature The compact ECDSA signature (r||s||v).
    /// @return The recovered signer address.
    /// @dev Reverts with `InvalidSignatureLength`, `SignatureInvalid`, or `UnknownSigner` on failure.
    function _verifySignature(bytes32 digest, bytes memory signature) private view returns (address) {
        require(signature.length == 65, InvalidSignatureLength(signature));

        address recovered = ECDSA.recover(digest, signature);
        require(recovered != address(0), SignatureInvalid(signature));
        require(_isAttestor[recovered], UnknownSigner(recovered));

        return recovered;
    }

    /// @notice Reverts if the client state is frozen.
    modifier notFrozen() {
        require(!clientState.isFrozen, FrozenClientState());
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
