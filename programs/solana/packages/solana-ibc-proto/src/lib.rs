//! Shared protobuf definitions for Solana IBC
//!
//! This crate provides protobuf-generated types used across Solana IBC programs and relayer.
//! By centralizing proto generation, we ensure type consistency and enable shared validation logic.

use anchor_lang::prelude::*;
use prost::Message;

// Re-export constrained types
pub use ibc_eureka_constrained_types::{
    ConstrainedBytes, ConstrainedError, ConstrainedString, ConstrainedVec, NonEmpty,
};

// Generated protobuf modules
#[allow(clippy::all)]
mod ibc_applications_gmp_v1 {
    include!(concat!(env!("OUT_DIR"), "/ibc.applications.gmp.v1.rs"));
}

pub mod solana {
    include!(concat!(env!("OUT_DIR"), "/solana.rs"));
}

// Re-export GMP types under familiar names
pub use ibc_applications_gmp_v1::{Acknowledgement as GmpAcknowledgement, GmpPacketData};
pub use solana::{GmpSolanaPayload, SolanaAccountMeta};

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

/// Maximum client ID length (64 bytes)
pub const MAX_CLIENT_ID_LENGTH: usize = 64;
/// Maximum sender address length (128 bytes)
pub const MAX_SENDER_LENGTH: usize = 128;
/// Maximum receiver address length (for Solana pubkey as string: 32-44 bytes)
pub const MAX_RECEIVER_LENGTH: usize = 64;
/// Maximum salt length (32 bytes)
pub const MAX_SALT_LENGTH: usize = 32;
/// Maximum memo length (256 bytes)
pub const MAX_MEMO_LENGTH: usize = 256;

pub type ClientId = ConstrainedString<1, MAX_CLIENT_ID_LENGTH>;
pub type Salt = ConstrainedBytes<0, MAX_SALT_LENGTH>;
pub type Sender = ConstrainedString<1, MAX_SENDER_LENGTH>;
/// Receiver can be empty to support native Cosmos module calls
/// where the receiver is implicitly determined by the GMP payload routing
pub type Receiver = ConstrainedString<0, MAX_RECEIVER_LENGTH>;
pub type Memo = ConstrainedString<0, MAX_MEMO_LENGTH>;
pub type Payload = NonEmpty<Vec<u8>>;

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

/// Validated GMP packet data with constrained types
///
/// All fields are validated and constrained at the type level.
/// The payload is validated to be non-empty (no maximum length constraint).
#[derive(Debug, Clone)]
pub struct ValidatedGmpPacketData {
    pub sender: Sender,
    pub receiver: Receiver,
    pub salt: Salt,
    pub payload: Payload,
    pub memo: Memo,
}

impl ValidatedGmpPacketData {
    /// Create a new validated GMP packet data from raw components
    ///
    /// Validates all fields against their constraints before construction.
    /// This is used for outgoing packets (send path) to ensure data integrity.
    pub fn new(
        sender: String,
        receiver: String,
        salt: Vec<u8>,
        payload: Vec<u8>,
        memo: String,
    ) -> core::result::Result<Self, GMPPacketError> {
        let sender = sender
            .try_into()
            .map_err(|_| GMPPacketError::InvalidSender)?;

        let receiver = receiver
            .try_into()
            .map_err(|_| GMPPacketError::InvalidReceiver)?;

        let salt = salt.try_into().map_err(|_| GMPPacketError::InvalidSalt)?;

        let payload = payload
            .try_into()
            .map_err(|_| GMPPacketError::EmptyPayload)?;

        let memo = memo.try_into().map_err(|_| GMPPacketError::MemoTooLong)?;

        Ok(Self {
            sender,
            receiver,
            salt,
            payload,
            memo,
        })
    }

    /// Encode to protobuf bytes
    ///
    /// This method directly encodes the validated data to protobuf format
    /// without requiring an intermediate GmpPacketData conversion.
    pub fn encode_to_vec(&self) -> Vec<u8> {
        let proto = GmpPacketData {
            sender: self.sender.to_string(),
            receiver: self.receiver.to_string(),
            salt: self.salt.to_vec(),
            payload: self.payload.to_vec(),
            memo: self.memo.to_string(),
        };

        proto.encode_to_vec()
    }
}

/// TryFrom implementation for decoding and validating from protobuf bytes
impl TryFrom<&[u8]> for ValidatedGmpPacketData {
    type Error = GMPPacketError;

    fn try_from(bytes: &[u8]) -> core::result::Result<Self, Self::Error> {
        use prost::Message;

        let packet = GmpPacketData::decode(bytes).map_err(|_| GMPPacketError::DecodeError)?;

        Self::new(
            packet.sender,
            packet.receiver,
            packet.salt,
            packet.payload,
            packet.memo,
        )
    }
}

/// Validated account metadata with Solana Pubkey
#[derive(Debug, Clone)]
pub struct ValidatedAccountMeta {
    pub pubkey: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
}

impl ValidatedAccountMeta {
    /// Convert to Anchor AccountMeta
    pub const fn to_account_meta(&self) -> AccountMeta {
        AccountMeta {
            pubkey: self.pubkey,
            is_signer: self.is_signer,
            is_writable: self.is_writable,
        }
    }
}

/// Validated GMP Solana payload with type-safe fields
#[derive(Debug, Clone)]
pub struct ValidatedGMPSolanaPayload {
    pub data: Vec<u8>,
    pub accounts: Vec<ValidatedAccountMeta>,
    pub payer_position: Option<u32>,
}

impl ValidatedGMPSolanaPayload {
    /// Convert accounts to Anchor AccountMeta format
    pub fn to_account_metas(&self) -> Vec<AccountMeta> {
        self.accounts
            .iter()
            .map(ValidatedAccountMeta::to_account_meta)
            .collect()
    }
}

impl TryFrom<NonEmpty<Vec<u8>>> for ValidatedGMPSolanaPayload {
    type Error = GmpValidationError;

    fn try_from(payload: NonEmpty<Vec<u8>>) -> core::result::Result<Self, Self::Error> {
        Self::try_from(payload.into_inner())
    }
}

impl TryFrom<Vec<u8>> for ValidatedGMPSolanaPayload {
    type Error = GmpValidationError;

    fn try_from(data: Vec<u8>) -> core::result::Result<Self, Self::Error> {
        use prost::Message;
        let payload = GmpSolanaPayload::decode(data.as_slice())
            .map_err(|_| GmpValidationError::DecodeError)?;

        // Validate data
        if payload.data.is_empty() {
            return Err(GmpValidationError::EmptyPayload);
        }

        let mut accounts = Vec::with_capacity(payload.accounts.len());
        for account in payload.accounts {
            let pubkey = Pubkey::try_from(&account.pubkey[..])
                .map_err(|_| GmpValidationError::InvalidAccountKey)?;
            accounts.push(ValidatedAccountMeta {
                pubkey,
                is_signer: account.is_signer,
                is_writable: account.is_writable,
            });
        }

        Ok(Self {
            data: payload.data,
            accounts,
            payer_position: payload.payer_position,
        })
    }
}

// Helper methods for encoding (we still need these for creating packets)
impl GmpPacketData {
    /// Encode to protobuf bytes
    pub fn encode_to_vec(&self) -> Vec<u8> {
        Message::encode_to_vec(self)
    }
}

impl GmpSolanaPayload {
    /// Encode to protobuf bytes
    pub fn encode_to_vec(&self) -> Vec<u8> {
        Message::encode_to_vec(self)
    }
}

// Helper methods for GmpAcknowledgement (encapsulate prost usage)
impl GmpAcknowledgement {
    /// Create new acknowledgement with result data
    pub const fn new(result: Vec<u8>) -> Self {
        Self { result }
    }

    /// Encode to protobuf bytes (compatible with Borsh `try_to_vec`)
    pub fn try_to_vec(&self) -> core::result::Result<Vec<u8>, prost::EncodeError> {
        let mut buf = Vec::new();
        Message::encode(self, &mut buf)?;
        Ok(buf)
    }

    /// Decode from protobuf bytes (compatible with Borsh `try_from_slice`)
    pub fn try_from_slice(data: &[u8]) -> core::result::Result<Self, prost::DecodeError> {
        <Self as Message>::decode(data)
    }
}
