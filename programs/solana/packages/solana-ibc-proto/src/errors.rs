/// Errors for GMP packet validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GMPPacketError {
    /// Failed to decode protobuf
    DecodeError,
    /// Sender validation failed
    InvalidSender,
    /// Receiver validation failed
    InvalidReceiver,
    /// Salt validation failed
    InvalidSalt,
    /// Payload is empty
    EmptyPayload,
    /// Payload validation failed
    InvalidPayload,
    /// Memo exceeds maximum length
    MemoTooLong,
}

impl core::fmt::Display for GMPPacketError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DecodeError => write!(f, "Failed to decode GMP packet"),
            Self::InvalidSender => write!(f, "Invalid sender address"),
            Self::InvalidReceiver => write!(f, "Invalid receiver address"),
            Self::InvalidSalt => write!(f, "Invalid salt"),
            Self::EmptyPayload => write!(f, "Empty payload"),
            Self::InvalidPayload => write!(f, "Invalid payload"),
            Self::MemoTooLong => write!(f, "Memo too long"),
        }
    }
}

/// Validation errors for GMP payloads
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmpValidationError {
    DecodeError,
    InvalidProgramId,
    EmptyPayload,
    TooManyAccounts,
    InvalidAccountKey,
}

impl core::fmt::Display for GmpValidationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DecodeError => write!(f, "Failed to decode GMP payload"),
            Self::InvalidProgramId => write!(f, "Invalid program ID (must be 32 bytes)"),
            Self::EmptyPayload => write!(f, "Empty payload data"),
            Self::TooManyAccounts => write!(f, "Too many accounts (max 32)"),
            Self::InvalidAccountKey => write!(f, "Invalid account key (must be 32 bytes)"),
        }
    }
}
