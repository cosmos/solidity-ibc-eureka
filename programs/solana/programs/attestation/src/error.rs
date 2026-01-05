use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Client is frozen due to misbehavior")]
    ClientFrozen,

    #[msg("No attestors provided")]
    NoAttestors,

    #[msg("Invalid quorum: must be greater than 0 and less than or equal to attestor count")]
    InvalidQuorum,

    #[msg("Duplicate attestor address detected")]
    DuplicateAttestor,

    #[msg("Signature verification failed")]
    InvalidSignature,

    #[msg("Recovered address is not in the trusted attestor set")]
    UnknownSigner,

    #[msg("Duplicate signature detected")]
    DuplicateSignature,

    #[msg("Signature threshold not met")]
    ThresholdNotMet,

    #[msg("Height mismatch between proof and consensus state")]
    HeightMismatch,

    #[msg("Path not found in attested packets")]
    PathNotFound,

    #[msg("Packet is not a member of the attested list")]
    NotMember,

    #[msg("Commitment is not zero (not a non-member)")]
    CommitmentNotZero,

    #[msg("Deserialization failed")]
    DeserializationFailed,

    #[msg("Invalid proof format")]
    InvalidProof,

    #[msg("Empty value provided")]
    EmptyValue,

    #[msg("Empty packets list")]
    EmptyPackets,

    #[msg("Invalid path length")]
    InvalidPathLength,

    #[msg("Invalid state: height and timestamp must be > 0")]
    InvalidState,

    #[msg("Empty signatures")]
    EmptySignatures,

    #[msg("Attestation verification failed")]
    AttestationVerificationFailed,

    #[msg("ABI decoding failed")]
    AbiDecodingFailed,
}
