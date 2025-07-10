use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Client is frozen")]
    ClientFrozen,
    #[msg("Client is already frozen")]
    ClientAlreadyFrozen,
    #[msg("Invalid header")]
    InvalidHeader,
    #[msg("Invalid height")]
    InvalidHeight,
    #[msg("Invalid proof")]
    InvalidProof,
    #[msg("Update client failed")]
    UpdateClientFailed,
    #[msg("Misbehaviour check failed")]
    MisbehaviourFailed,
    #[msg("Verification failed")]
    VerificationFailed,
    #[msg("Membership verification failed")]
    MembershipVerificationFailed,
    #[msg("Non-membership verification failed")]
    NonMembershipVerificationFailed,
    #[msg("Insufficient time delay")]
    InsufficientTimeDelay,
    #[msg("Insufficient block delay")]
    InsufficientBlockDelay,
    #[msg("Invalid value for non-membership proof")]
    InvalidValue,
    #[msg("Invalid chain ID")]
    InvalidChainId,
    #[msg("Invalid trust level")]
    InvalidTrustLevel,
    #[msg("Invalid periods: trusting period must be positive and less than unbonding period")]
    InvalidPeriods,
    #[msg("Invalid max clock drift")]
    InvalidMaxClockDrift,
    #[msg("Serialization error")]
    SerializationError,
}