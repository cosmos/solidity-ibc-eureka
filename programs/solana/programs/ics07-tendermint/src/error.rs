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
    #[msg("Height mismatch: expected height does not match provided height")]
    HeightMismatch,

    // Header and proof errors
    #[msg("Invalid header: failed to deserialize or validate header")]
    InvalidHeader,
    #[msg("Invalid proof: proof validation failed")]
    InvalidProof,
    #[msg("Proof height not found: no consensus state at the specified height")]
    ProofHeightNotFound,

    // Update errors
    #[msg("Update client failed: header verification failed")]
    UpdateClientFailed,
    #[msg("Header verification failed: cryptographic validation failed after successful deserialization")]
    HeaderVerificationFailed,

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
    #[msg("Invalid length: state root or next validators hash is not 32 bytes long")]
    InvalidRootLength,

    // Chunking errors
    #[msg("Invalid chunk count: unexpected number of chunk accounts")]
    InvalidChunkCount,
    #[msg("Metadata already initialized: cannot reinitialize upload metadata")]
    MetadataAlreadyInitialized,
    #[msg("Invalid chunk account: chunk account PDA mismatch")]
    InvalidChunkAccount,
    #[msg("Invalid chunk index: chunk index out of bounds")]
    InvalidChunkIndex,
    #[msg("Too many chunks: exceeds maximum supported chunks")]
    TooManyChunks,
    #[msg("Chunk data too large: exceeds maximum chunk size")]
    ChunkDataTooLarge,

    // Other errors
    #[msg("Serialization error: failed to serialize/deserialize data")]
    SerializationError,
    #[msg("Account validation failed: invalid account or PDA")]
    AccountValidationFailed,
}
