// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title Native CometBFT Light Client Errors
interface ICometBFTClientErrors {
    error ConsensusStateNotFound(uint64 revisionNumber, uint64 revisionHeight);
    error ConsensusStateHashMismatch(bytes32 expected, bytes32 actual);
    error FrozenClientState();
    error FeatureNotSupported();
    error InvalidMisbehaviour();
    error EmptyMembershipValue();
    error InvalidICS23Proof();
    error UnsupportedICS23ProofType(uint8 proofType);
    error HeaderHeightNotIncreasing(uint64 trustedHeight, uint64 newHeight);
    error RevisionNumberMismatch(uint64 expected, uint64 actual);
    error InvalidClientState();
    error InvalidHeaderTimestampNanos(uint32 nanos);
    error InvalidCommitTimestampNanos(uint256 index, uint32 nanos);
    error ChainIdMismatch(string expected, string actual);
    error HeaderTimeNotIncreasing(uint128 trustedTime, uint128 newTime);
    error TrustedConsensusStateExpired(uint256 expiresAt, uint256 nowSeconds);
    error HeaderFromFuture(uint256 headerTime, uint256 nowSeconds, uint256 maxClockDrift);
    error ValidatorSetHashMismatch(bytes32 expected, bytes32 actual);
    error NextValidatorSetHashMismatch(bytes32 expected, bytes32 actual);
    error TrustedValidatorSetNotFound(uint64 revisionNumber, uint64 revisionHeight);
    error TrustedValidatorSetHashMismatch(bytes32 expected, bytes32 actual);
    error AdjacentValidatorHashMismatch(bytes32 trustedNextValidatorsHash, bytes32 newValidatorsHash);
    error HeaderCommitHashMismatch(bytes32 headerHash, bytes32 commitBlockHash);
    error InvalidCommitHeight(uint64 expected, uint64 actual);
    error InvalidCommitBlockID();
    error InvalidAbsentCommitSignature(uint256 index);
    error InvalidValidatorSet();
    error InvalidValidatorOrdering(uint256 index);
    error InvalidValidatorPower(uint256 index);
    error InvalidValidatorPubKey(uint256 index);
    error InvalidValidatorPubKeyWitness(uint256 index);
    error InvalidCommitSignaturesLength(uint256 expected, uint256 actual);
    error ValidatorAddressMismatch(uint256 index, address expected, address actual);
    error InvalidBlockIDFlag(uint8 flag);
    error InvalidSignatureLength(uint256 length);
    error InvalidSignatureV(uint8 v);
    error InvalidSignatureS(bytes32 s);
    error SignatureInvalid(bytes signature);
    error SignatureSignerMismatch(uint256 index, address expected, address actual);
    error NotEnoughVotingPower(uint256 got, uint256 needed);
    error NotEnoughTrustedVotingPower(uint256 got, uint256 needed);
    error DuplicateTrustedValidatorSignature(uint256 commitIndex, uint256 trustedValidatorIndex);
}
