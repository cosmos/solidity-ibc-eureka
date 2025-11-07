//! Shared protobuf definitions for Solana IBC
//!
//! This crate provides protobuf-generated types used across Solana IBC programs and relayer.
//! By centralizing proto generation, we ensure type consistency and enable shared validation logic.

use anchor_lang::prelude::*;

// Re-export prost so generated code can find it
pub use prost;

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
    pub program_id: Pubkey,
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

// Direct validation and encoding implementation for GmpPacketData
use solana_ibc_types::{GMPPacketError, Salt, Sender, ValidatedGmpPacketData};

const MAX_PAYLOAD_LENGTH: usize = 10 * 1024; // 10KB
const MAX_MEMO_LENGTH: usize = 256;

impl GmpPacketData {
    /// Encode to protobuf bytes
    pub fn encode_to_vec(&self) -> Vec<u8> {
        use prost::Message;
        Message::encode_to_vec(self)
    }

    /// Decode from protobuf bytes
    pub fn decode_from_bytes(data: &[u8]) -> core::result::Result<Self, prost::DecodeError> {
        use prost::Message;
        <Self as Message>::decode(data)
    }

    /// Decode from protobuf bytes and validate in one step
    pub fn decode_and_validate(
        bytes: &[u8],
    ) -> core::result::Result<ValidatedGmpPacketData, GMPPacketError> {
        use prost::Message;
        let packet = <Self as Message>::decode(bytes).map_err(|_| GMPPacketError::DecodeError)?;
        packet.validate()
    }

    /// Validate packet data and convert to typed form
    pub fn validate(self) -> core::result::Result<ValidatedGmpPacketData, GMPPacketError> {
        // Validate and construct typed Sender
        let sender = Sender::new(self.sender).map_err(|_| GMPPacketError::InvalidSender)?;

        // Validate and construct typed Salt
        let salt = Salt::new(self.salt).map_err(|_| GMPPacketError::InvalidSalt)?;

        // Validate payload length
        if self.payload.is_empty() {
            return Err(GMPPacketError::EmptyPayload);
        }
        if self.payload.len() > MAX_PAYLOAD_LENGTH {
            return Err(GMPPacketError::PayloadTooLong);
        }

        // Validate memo length
        if self.memo.len() > MAX_MEMO_LENGTH {
            return Err(GMPPacketError::MemoTooLong);
        }

        Ok(ValidatedGmpPacketData {
            sender,
            receiver: self.receiver,
            salt,
            payload: self.payload,
            memo: self.memo,
        })
    }
}

// Helper methods for GmpSolanaPayload (encapsulate prost usage)
impl GmpSolanaPayload {
    /// Encode to protobuf bytes
    pub fn encode_to_vec(&self) -> Vec<u8> {
        use prost::Message;
        Message::encode_to_vec(self)
    }

    /// Decode from protobuf bytes
    pub fn decode_from_bytes(data: &[u8]) -> core::result::Result<Self, prost::DecodeError> {
        use prost::Message;
        <Self as Message>::decode(data)
    }

    /// Parse and validate GMP Solana payload from Protobuf-encoded bytes
    pub fn decode_and_validate(
        data: &[u8],
    ) -> core::result::Result<ValidatedGMPSolanaPayload, GmpValidationError> {
        let payload = Self::decode_from_bytes(data).map_err(|_| GmpValidationError::DecodeError)?;

        // Validate and convert program_id
        if payload.program_id.len() != 32 {
            return Err(GmpValidationError::InvalidProgramId);
        }
        let program_id = Pubkey::try_from(payload.program_id.as_slice())
            .map_err(|_| GmpValidationError::InvalidProgramId)?;

        // Validate data
        if payload.data.is_empty() {
            return Err(GmpValidationError::EmptyPayload);
        }

        // Validate and convert accounts
        if payload.accounts.len() > 32 {
            return Err(GmpValidationError::TooManyAccounts);
        }
        let mut accounts = Vec::with_capacity(payload.accounts.len());
        for account in payload.accounts {
            if account.pubkey.len() != 32 {
                return Err(GmpValidationError::InvalidAccountKey);
            }
            let pubkey = Pubkey::try_from(account.pubkey.as_slice())
                .map_err(|_| GmpValidationError::InvalidAccountKey)?;
            accounts.push(ValidatedAccountMeta {
                pubkey,
                is_signer: account.is_signer,
                is_writable: account.is_writable,
            });
        }

        Ok(ValidatedGMPSolanaPayload {
            program_id,
            data: payload.data,
            accounts,
            payer_position: payload.payer_position,
        })
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
        use prost::Message;
        let mut buf = Vec::new();
        Message::encode(self, &mut buf)?;
        Ok(buf)
    }

    /// Decode from protobuf bytes (compatible with Borsh `try_from_slice`)
    pub fn try_from_slice(data: &[u8]) -> core::result::Result<Self, prost::DecodeError> {
        use prost::Message;
        <Self as Message>::decode(data)
    }
}
