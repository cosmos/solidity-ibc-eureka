use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Client is frozen")]
    ClientFrozen,

    #[msg("Invalid client ID: cannot be empty")]
    InvalidClientId,

    #[msg("Invalid attestor addresses: must have at least one attestor")]
    InvalidAttestorAddresses,

    #[msg("Invalid min required signatures: must be positive and not exceed attestor count")]
    InvalidMinRequiredSigs,

    #[msg("Invalid height: height cannot be zero")]
    InvalidHeight,

    #[msg("Invalid state: height and timestamp must be non-zero")]
    InvalidState,

    #[msg("Height mismatch: expected height does not match proof height")]
    HeightMismatch,

    #[msg("Invalid proof: proof validation failed")]
    InvalidProof,

    #[msg("Invalid signature: signature recovery failed")]
    InvalidSignature,

    #[msg("Unknown address recovered: signer not in trusted attestor set")]
    UnknownAddressRecovered,

    #[msg("Too few signatures: minimum required signatures not met")]
    TooFewSignatures,

    #[msg("Duplicate signature: same signature provided multiple times")]
    DuplicateSignature,

    #[msg("Invalid attestation data: failed to decode")]
    InvalidAttestationData,

    #[msg("Path not found: commitment path not in attestation")]
    PathNotFound,

    #[msg("Commitment mismatch: value does not match attested commitment")]
    CommitmentMismatch,

    #[msg("Non-zero commitment: expected zero commitment for non-membership")]
    NonZeroCommitment,

    #[msg("Empty value: membership proof value cannot be empty")]
    EmptyValue,

    #[msg("Invalid path: expected path length of 1")]
    InvalidPathLength,

    #[msg("Unauthorized: caller does not have required role")]
    UnauthorizedRole,

    #[msg("Serialization error: failed to serialize/deserialize data")]
    SerializationError,

    #[msg("Arithmetic overflow detected")]
    ArithmeticOverflow,

    #[msg("Empty attestation: no packets in attestation data")]
    EmptyAttestation,

    #[msg("No signatures provided")]
    NoSignatures,
}
