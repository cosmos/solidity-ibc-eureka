// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title Attestor Light Client Errors
interface IAttestorErrors {
    error ClientFrozen();
    error InsufficientSignatures(uint256 have, uint256 need);
    error DuplicateSigner(address signer);
    error UnknownSigner(address signer);
    error InvalidSignature();
    error ConsensusStateNotFound();
    error TimestampMismatch();
    error NotMonotonic();
    error FeatureNotSupported();
    error EmptyValue();
}


