//! ICS27 GMP (General Message Passing) types for PDA derivation
//!
//! These types are shared between the ICS27 GMP program and relayer
//! to ensure consistent PDA derivation across the system.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hash;

/// Maximum client ID length (64 bytes)
pub const MAX_CLIENT_ID_LENGTH: usize = 64;
/// Maximum sender address length (128 bytes)
pub const MAX_SENDER_LENGTH: usize = 128;
/// Maximum salt length (32 bytes)
pub const MAX_SALT_LENGTH: usize = 32;

/// Errors for domain type validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GMPAccountError {
    ClientIdTooLong,
    SenderTooLong,
    SaltTooLong,
}

/// Validated client ID (max 64 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientId(String);

impl ClientId {
    pub fn new(s: impl Into<String>) -> core::result::Result<Self, GMPAccountError> {
        let s = s.into();
        if s.len() > MAX_CLIENT_ID_LENGTH {
            return Err(GMPAccountError::ClientIdTooLong);
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ClientId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Validated sender address (max 128 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sender(String);

impl Sender {
    pub fn new(s: impl Into<String>) -> core::result::Result<Self, GMPAccountError> {
        let s = s.into();
        if s.is_empty() {
            return Err(GMPAccountError::SenderTooLong);
        }
        if s.len() > MAX_SENDER_LENGTH {
            return Err(GMPAccountError::SenderTooLong);
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Sender {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Validated salt (max 32 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Salt(Vec<u8>);

impl Salt {
    pub fn new(bytes: impl Into<Vec<u8>>) -> core::result::Result<Self, GMPAccountError> {
        let bytes = bytes.into();
        if bytes.len() > MAX_SALT_LENGTH {
            return Err(GMPAccountError::SaltTooLong);
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for Salt {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// GMP account identifier for PDA derivation
///
/// This type provides stateless PDA derivation for cross-chain account abstraction.
/// Each unique combination of (client_id, sender, salt) derives a unique GMP account PDA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GMPAccount {
    pub client_id: ClientId,
    pub sender: Sender,
    pub salt: Salt,
    pub sender_hash: [u8; 32],
    pub pda: Pubkey,
    pub bump: u8,
}

impl GMPAccount {
    /// Seed for individual account PDAs in the GMP program
    pub const SEED: &'static [u8] = b"gmp_account";

    /// Create a new GMPAccount with PDA derivation
    ///
    /// Accepts validated types, so no validation needed - construction cannot fail
    pub fn new(client_id: ClientId, sender: Sender, salt: Salt, program_id: &Pubkey) -> Self {
        // Calculate hash and PDA
        let sender_hash = hash(sender.as_str().as_bytes()).to_bytes();
        let (pda, bump) = Pubkey::find_program_address(
            &[
                Self::SEED,
                client_id.as_str().as_bytes(),
                &sender_hash,
                salt.as_bytes(),
            ],
            program_id,
        );

        Self {
            client_id,
            sender,
            salt,
            sender_hash,
            pda,
            bump,
        }
    }

    /// Get the derived PDA and bump
    pub fn pda(&self) -> (Pubkey, u8) {
        (self.pda, self.bump)
    }

    /// Create signer seeds for use with invoke_signed
    pub fn to_signer_seeds(&self) -> SignerSeeds {
        SignerSeeds {
            client_id: self.client_id.clone(),
            sender_hash: self.sender_hash,
            salt: self.salt.clone(),
            bump: self.bump,
        }
    }

    /// Invoke a cross-program instruction with this GMP account as signer
    pub fn invoke_signed(
        &self,
        instruction: &anchor_lang::solana_program::instruction::Instruction,
        account_infos: &[anchor_lang::prelude::AccountInfo],
    ) -> Result<()> {
        let seeds = self.to_signer_seeds();
        let seeds_slices = seeds.as_slices();
        anchor_lang::solana_program::program::invoke_signed(
            instruction,
            account_infos,
            &[&seeds_slices],
        )
        .map_err(|e| e.into())
    }
}

/// Signer seeds wrapper for invoke_signed
pub struct SignerSeeds {
    client_id: ClientId,
    sender_hash: [u8; 32],
    salt: Salt,
    bump: u8,
}

impl SignerSeeds {
    /// Get seeds as slices for invoke_signed
    pub fn as_slices(&self) -> [&[u8]; 5] {
        [
            GMPAccount::SEED,
            self.client_id.as_str().as_bytes(),
            &self.sender_hash,
            self.salt.as_bytes(),
            std::slice::from_ref(&self.bump),
        ]
    }
}

/// Marker type for GMP application state PDA
pub struct GMPAppState;

impl GMPAppState {
    /// Seed for the main GMP application state PDA
    /// Follows the standard IBC app pattern: [`APP_STATE_SEED`, `port_id`]
    pub const SEED: &'static [u8] = b"app_state";
}

/// Maximum payload length (1MB)
pub const MAX_PAYLOAD_LENGTH: usize = 1_048_576;
/// Maximum memo length (256 bytes)
pub const MAX_MEMO_LENGTH: usize = 256;

/// Errors for GMP packet validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GMPPacketError {
    /// Failed to decode protobuf
    DecodeError,
    /// Sender validation failed
    InvalidSender,
    /// Salt validation failed
    InvalidSalt,
    /// Payload is empty
    EmptyPayload,
    /// Payload exceeds maximum length
    PayloadTooLong,
    /// Memo exceeds maximum length
    MemoTooLong,
}

/// Validated GMP packet data with constrained types
#[derive(Debug, Clone)]
pub struct ValidatedGmpPacketData {
    pub sender: Sender,
    pub receiver: String,
    pub salt: Salt,
    pub payload: Vec<u8>,
    pub memo: String,
}

/// Trait for validating protobuf `GmpPacketData` into typed `ValidatedGmpPacketData`
///
/// Implement this trait on your crate's protobuf-generated `GmpPacketData` type
/// to get validation and conversion to the shared `ValidatedGmpPacketData` type.
///
/// # Example
///
/// ```ignore
/// impl ValidateGmpPacketData for my_proto::GmpPacketData {
///     fn into_fields(self) -> (String, String, Vec<u8>, Vec<u8>, String) {
///         (self.sender, self.receiver, self.salt, self.payload, self.memo)
///     }
/// }
///
/// // Usage:
/// let validated = GmpPacketData::decode_and_validate(bytes)?;
/// ```
pub trait ValidateGmpPacketData: prost::Message + Default + Sized {
    /// Extract fields from protobuf packet data
    ///
    /// Returns: (sender, receiver, salt, payload, memo)
    fn into_fields(self) -> (String, String, Vec<u8>, Vec<u8>, String);

    /// Decode from protobuf bytes and validate in one step
    fn decode_and_validate(
        bytes: &[u8],
    ) -> core::result::Result<ValidatedGmpPacketData, GMPPacketError> {
        let packet = Self::decode(bytes).map_err(|_| GMPPacketError::DecodeError)?;
        packet.validate()
    }

    /// Validate packet data and convert to typed form
    fn validate(self) -> core::result::Result<ValidatedGmpPacketData, GMPPacketError> {
        let (sender, receiver, salt, payload, memo) = self.into_fields();

        // Validate and construct typed Sender
        let sender = Sender::new(sender).map_err(|_| GMPPacketError::InvalidSender)?;

        // Validate and construct typed Salt
        let salt = Salt::new(salt).map_err(|_| GMPPacketError::InvalidSalt)?;

        // Validate payload length
        if payload.is_empty() {
            return Err(GMPPacketError::EmptyPayload);
        }
        if payload.len() > MAX_PAYLOAD_LENGTH {
            return Err(GMPPacketError::PayloadTooLong);
        }

        // Validate memo length
        if memo.len() > MAX_MEMO_LENGTH {
            return Err(GMPPacketError::MemoTooLong);
        }

        Ok(ValidatedGmpPacketData {
            sender,
            receiver,
            salt,
            payload,
            memo,
        })
    }
}
