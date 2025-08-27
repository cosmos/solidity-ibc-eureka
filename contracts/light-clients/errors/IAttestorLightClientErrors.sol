// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title IAttestorLightClientErrors
/// @notice Error interface for the attestor-based IBC light client
interface IAttestorLightClientErrors {
    /// @notice Reverts when the attestor set is empty during initialization.
    error NoAttestors();
    /// @notice Reverts when the quorum threshold is zero or exceeds the attestor count.
    /// @param minRequired The provided minimum required signatures.
    /// @param attestorCount The size of the configured attestor set.
    error BadQuorum(uint8 minRequired, uint256 attestorCount);
    /// @notice Reverts when an action is attempted on a frozen client state.
    error FrozenClientState();
    /// @notice Reverts for functions that are out of scope for this implementation.
    error FeatureNotSupported();
    /// @notice Missing trusted timestamp for the given height.
    /// @param height The height that has no associated timestamp.
    error ConsensusTimestampNotFound(uint64 height);
    /// @notice Conflicting timestamp for an already stored height.
    /// @param height The height that already has a timestamp.
    /// @param storedTimestamp The previously stored timestamp.
    /// @param providedTimestamp The new, conflicting timestamp.
    error ConflictingTimestamp(
        uint64 height,
        uint64 storedTimestamp,
        uint64 providedTimestamp
    );
    /// @notice ECDSA signature has an invalid length.
    /// @param signature The invalid signature.
    error InvalidSignatureLength(bytes signature);
    /// @notice ECDSA signature failed to recover a valid signer.
    /// @param signature The invalid signature.
    error SignatureInvalid(bytes signature);
    /// @notice The recovered signer is not part of the attestor set.
    /// @param signer Address of the unknown signer.
    error UnknownSigner(address signer);
    /// @notice The recovered signer appears more than once among the signatures.
    /// @param signer Address of the duplicate signer.
    error DuplicateSigner(address signer);
    /// @notice The number of valid unique signatures is below the required threshold.
    /// @param validSigners Number of valid, unique attestor signatures.
    /// @param minRequired The minimum required signatures.
    error ThresholdNotMet(uint256 validSigners, uint8 minRequired);
    /// @notice The provided membership value was empty.
    error EmptyValue();
    /// @notice The provided packet commitments were empty.
    error EmptyPacketCommitments();
    /// @notice The provided value is not a member of the attested set.
    error NotMember();
    /// @notice The attested height did not match the provided proof height.
    /// @param expected The expected height from the proofHeight field.
    /// @param provided The height that was present in the attested payload.
    error HeightMismatch(uint64 expected, uint64 provided);
    /// @notice The attested state is invalid.
    /// @param height The height of the attested state.
    /// @param timestamp The timestamp of the attested state.
    error InvalidState(uint64 height, uint64 timestamp);
    /// @notice The provided signatures were empty.
    error EmptySignatures();
}
