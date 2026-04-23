// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IBesuLightClientErrors {
    error InvalidRevisionNumber(uint64 providedRevisionNumber);
    error InvalidHeaderFormat(uint256 itemsLength);
    error InvalidExtraDataFormat(uint256 itemsLength);
    error InvalidHeaderHeight();
    error HeaderFromFuture(uint256 currentTimestamp, uint256 headerTimestamp, uint256 maxClockDrift);
    error ConsensusStateNotFound(uint64 revisionHeight);
    error ConsensusStateExpired(uint64 trustedTimestamp, uint256 currentTimestamp, uint64 trustingPeriod);
    error InvalidMixHash(bytes32 actualMixHash);
    error InvalidDifficulty(uint256 actualDifficulty);
    error InvalidNonce(bytes actualNonce);
    error InvalidOmmersHash(bytes32 actualOmmersHash);
    error EmptyValidatorSet();
    error InvalidValidatorAddressLength(uint256 length);
    error InvalidValidatorAddress(address validator);
    error DuplicateValidator(address validator);
    error DuplicateCommitSealSigner(address signer);
    error InvalidECDSASignatureLength(uint256 length);
    error InvalidCommitSeal();
    error InsufficientTrustedValidatorOverlap(uint256 actual, uint256 required);
    error InsufficientValidatorQuorum(uint256 actual, uint256 required);
    error InvalidPathLength(uint256 expectedLength, uint256 actualLength);
    error InvalidValueLength(uint256 expectedLength, uint256 actualLength);
    error InvalidCommitmentValue(bytes32 expectedValue, bytes32 actualValue);
    error ValueExists(bytes32 actualValue);
    error ConflictingConsensusState(uint64 revisionHeight);
    error UnsupportedMisbehaviour();
}
