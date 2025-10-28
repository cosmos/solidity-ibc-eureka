use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    // Client state errors
    #[msg("Client is frozen")]
    ClientFrozen,
    #[msg("Client is already frozen")]
    ClientAlreadyFrozen,
    #[msg("Invalid chain ID: cannot be empty")]
    InvalidChainId,
    #[msg("Invalid trust level: numerator must be > 0 and <= denominator")]
    InvalidTrustLevel,
    #[msg("Invalid periods: trusting period must be positive and less than unbonding period")]
    InvalidPeriods,
    #[msg("Invalid max clock drift: must be positive")]
    InvalidMaxClockDrift,

    // Height errors
    #[msg("Invalid height: height cannot be zero")]
    InvalidHeight,

    // Header and proof errors
    #[msg("Invalid header: failed to deserialize or validate header")]
    InvalidHeader,
    #[msg("Invalid proof: proof validation failed")]
    InvalidProof,

    // Update errors
    #[msg("Update client failed: header verification failed")]
    UpdateClientFailed,

    // Misbehaviour errors
    #[msg("Misbehaviour detected: conflicting consensus state at same height")]
    MisbehaviourConflictingState,
    #[msg("Misbehaviour detected: non-increasing timestamp")]
    MisbehaviourNonIncreasingTime,
    #[msg("Misbehaviour check failed: invalid misbehaviour proof")]
    MisbehaviourCheckFailed,

    // Verification errors
    #[msg("Membership verification failed: proof does not match commitment")]
    MembershipVerificationFailed,
    #[msg("Membership verification failed: value cannot be empty")]
    MembershipEmptyValue,
    #[msg("Non-membership verification failed: key exists when it should not")]
    NonMembershipVerificationFailed,
    #[msg("Invalid value: non-membership proof must have empty value")]
    InvalidValue,

    // Delay errors
    #[msg("Time and block delay must be zero")]
    NonZeroDelay,

    // Consensus state errors
    #[msg("Consensus state not found at the specified height")]
    ConsensusStateNotFound,

    // Chunking errors
    #[msg("Invalid chunk count: unexpected number of chunk accounts")]
    InvalidChunkCount,
    #[msg("Invalid chunk account: chunk account PDA mismatch")]
    InvalidChunkAccount,
    #[msg("Chunk data too large: exceeds maximum chunk size")]
    ChunkDataTooLarge,
    #[msg("Missing account in remaining accounts")]
    MissingAccount,
    #[msg("Invalid account")]
    InvalidAccount,

    // Other errors
    #[msg("Serialization error: failed to serialize/deserialize data")]
    SerializationError,
    #[msg("Account validation failed: invalid account or PDA")]
    AccountValidationFailed,
}
