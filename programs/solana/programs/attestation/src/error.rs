use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Client state is frozen")]
    FrozenClientState,

    #[msg("Invalid client ID: cannot be empty")]
    InvalidClientId,

    #[msg("No attestors provided")]
    NoAttestors,

    #[msg("Bad quorum: min required signatures must be positive and not exceed attestor count")]
    BadQuorum,

    #[msg("Invalid height: height cannot be zero")]
    InvalidHeight,

    #[msg("Invalid timestamp: timestamp cannot be zero")]
    InvalidTimestamp,

    #[msg("Invalid state: height and timestamp must be non-zero")]
    InvalidState,

    #[msg("Height mismatch: expected height does not match proof height")]
    HeightMismatch,

    #[msg("Invalid proof: proof validation failed")]
    InvalidProof,

    #[msg("Invalid signature: signature recovery failed")]
    InvalidSignature,

    #[msg("Unknown signer: address not in trusted attestor set")]
    UnknownSigner,

    #[msg("Threshold not met: minimum required signatures not provided")]
    ThresholdNotMet,

    #[msg("Duplicate signer: same signer provided multiple times")]
    DuplicateSigner,

    #[msg("Invalid attestation data: failed to decode")]
    InvalidAttestationData,

    #[msg("Not member: commitment path not in attestation")]
    NotMember,

    #[msg("Commitment mismatch: value does not match attested commitment")]
    CommitmentMismatch,

    #[msg("Non-zero commitment: expected zero commitment for non-membership")]
    NonZeroCommitment,

    #[msg("Empty value: membership proof value cannot be empty")]
    EmptyValue,

    #[msg("Invalid path length: expected path length of 1")]
    InvalidPathLength,

    #[msg("Unauthorized: caller does not have required role")]
    UnauthorizedRole,

    #[msg("Serialization error: failed to serialize/deserialize data")]
    SerializationError,

    #[msg("Arithmetic overflow detected")]
    ArithmeticOverflow,

    #[msg("Empty attestation: no packets in attestation data")]
    EmptyAttestation,

    #[msg("Empty signatures: no signatures provided")]
    EmptySignatures,

    #[msg("Misbehaviour detected: conflicting timestamp for existing height")]
    Misbehaviour,

    #[msg("Consensus timestamp not found: no trusted timestamp at requested height")]
    ConsensusTimestampNotFound,
}
