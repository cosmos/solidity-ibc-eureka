//! Shared protobuf definitions for Solana IBC
//!
//! This crate provides protobuf-generated types used across Solana IBC programs and relayer.
//! By centralizing proto generation, we ensure type consistency and enable shared validation logic.

use anchor_lang::prelude::*;

// Re-export Protobuf trait for users
pub use ibc_proto::Protobuf;

// Re-export constrained types
pub use ibc_eureka_constrained_types::{
    ConstrainedBytes, ConstrainedError, ConstrainedString, ConstrainedVec, NonEmpty,
};

mod errors;
pub use errors::{GMPPacketError, GmpValidationError};

// Generated protobuf modules
#[allow(clippy::all)]
mod ibc_applications_gmp_v1 {
    include!(concat!(env!("OUT_DIR"), "/ibc.applications.gmp.v1.rs"));
}

pub mod solana {
    include!(concat!(env!("OUT_DIR"), "/solana.rs"));
}

pub use ibc_applications_gmp_v1::{
    Acknowledgement as GmpAcknowledgement, GmpPacketData as RawGmpPacketData,
};
pub use solana::{
    GmpSolanaPayload as RawGmpSolanaPayload, SolanaAccountMeta as RawSolanaAccountMeta,
};

impl Protobuf<RawGmpPacketData> for RawGmpPacketData {}
impl Protobuf<RawGmpSolanaPayload> for RawGmpSolanaPayload {}
impl Protobuf<RawSolanaAccountMeta> for RawSolanaAccountMeta {}

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

/// Domain type for GMP packet data with validated and constrained fields.
///
/// All fields are validated and constrained at the type level.
/// The payload is validated to be non-empty (no maximum length constraint).
#[derive(Debug, Clone)]
pub struct GmpPacketData {
    pub sender: Sender,
    pub receiver: Receiver,
    pub salt: Salt,
    pub payload: Payload,
    pub memo: Memo,
}

impl Protobuf<RawGmpPacketData> for GmpPacketData {}

impl TryFrom<RawGmpPacketData> for GmpPacketData {
    type Error = GMPPacketError;

    fn try_from(raw: RawGmpPacketData) -> core::result::Result<Self, Self::Error> {
        let sender = raw
            .sender
            .try_into()
            .map_err(|_| GMPPacketError::InvalidSender)?;

        let receiver = raw
            .receiver
            .try_into()
            .map_err(|_| GMPPacketError::InvalidReceiver)?;

        let salt = raw
            .salt
            .try_into()
            .map_err(|_| GMPPacketError::InvalidSalt)?;

        let payload = raw
            .payload
            .try_into()
            .map_err(|_| GMPPacketError::EmptyPayload)?;

        let memo = raw
            .memo
            .try_into()
            .map_err(|_| GMPPacketError::MemoTooLong)?;

        Ok(Self {
            sender,
            receiver,
            salt,
            payload,
            memo,
        })
    }
}

impl From<GmpPacketData> for RawGmpPacketData {
    fn from(packet: GmpPacketData) -> Self {
        Self {
            sender: packet.sender.into_string(),
            receiver: packet.receiver.into_string(),
            salt: packet.salt.into_vec(),
            payload: packet.payload.into_inner(),
            memo: packet.memo.into_string(),
        }
    }
}

/// Domain type for Solana account metadata with validated Pubkey
#[derive(Debug, Clone)]
pub struct SolanaAccountMeta {
    pub pubkey: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
}

impl From<&SolanaAccountMeta> for anchor_lang::prelude::AccountMeta {
    fn from(meta: &SolanaAccountMeta) -> Self {
        Self {
            pubkey: meta.pubkey,
            is_signer: meta.is_signer,
            is_writable: meta.is_writable,
        }
    }
}

/// Domain type for GMP Solana payload with type-safe fields
#[derive(Debug, Clone)]
pub struct GmpSolanaPayload {
    pub data: Vec<u8>,
    pub accounts: Vec<SolanaAccountMeta>,
    pub payer_position: Option<u32>,
}

impl GmpSolanaPayload {
    /// Convert accounts to Anchor AccountMeta format
    pub fn to_account_metas(&self) -> Vec<anchor_lang::prelude::AccountMeta> {
        self.accounts.iter().map(Into::into).collect()
    }
}

impl From<GmpSolanaPayload> for RawGmpSolanaPayload {
    fn from(payload: GmpSolanaPayload) -> Self {
        Self {
            data: payload.data,
            accounts: payload
                .accounts
                .into_iter()
                .map(|acc| RawSolanaAccountMeta {
                    pubkey: acc.pubkey.to_bytes().to_vec(),
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                })
                .collect(),
            payer_position: payload.payer_position,
        }
    }
}

impl Protobuf<RawGmpSolanaPayload> for GmpSolanaPayload {}

impl TryFrom<RawGmpSolanaPayload> for GmpSolanaPayload {
    type Error = GmpValidationError;

    fn try_from(raw: RawGmpSolanaPayload) -> core::result::Result<Self, Self::Error> {
        // Validate data
        if raw.data.is_empty() {
            return Err(GmpValidationError::EmptyPayload);
        }

        let mut accounts = Vec::with_capacity(raw.accounts.len());
        for account in raw.accounts {
            let pubkey = Pubkey::try_from(&account.pubkey[..])
                .map_err(|_| GmpValidationError::InvalidAccountKey)?;
            accounts.push(SolanaAccountMeta {
                pubkey,
                is_signer: account.is_signer,
                is_writable: account.is_writable,
            });
        }

        Ok(Self {
            data: raw.data,
            accounts,
            payer_position: raw.payer_position,
        })
    }
}

// GmpAcknowledgement is a simple protobuf type that doesn't need validation
impl Protobuf<GmpAcknowledgement> for GmpAcknowledgement {}

impl GmpAcknowledgement {
    /// Create new acknowledgement with result data
    pub const fn new(result: Vec<u8>) -> Self {
        Self { result }
    }
}
