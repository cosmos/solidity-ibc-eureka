use crate::constants::*;
use crate::errors::GMPError;
use anchor_lang::prelude::*;

/// Main GMP application state
#[account]
#[derive(InitSpace)]
pub struct GMPAppState {
    /// ICS26 Router program that manages this app
    pub router_program: Pubkey,

    /// Administrative authority
    pub authority: Pubkey,

    /// Program version for upgrades
    pub version: u8,

    /// Emergency pause flag
    pub paused: bool,

    /// PDA bump seed
    pub bump: u8,
}

impl GMPAppState {
    pub const SEED: &'static [u8] = solana_ibc_types::GMPAppState::SEED;

    /// Get signer seeds for this app state
    /// Seeds: [`b"app_state`", `GMP_PORT_ID.as_bytes()`, bump]
    pub fn signer_seeds(&self) -> Vec<Vec<u8>> {
        vec![
            Self::SEED.to_vec(),
            GMP_PORT_ID.as_bytes().to_vec(),
            vec![self.bump],
        ]
    }

    /// Check if app is operational
    pub fn can_operate(&self) -> Result<()> {
        require!(!self.paused, GMPError::AppPaused);
        Ok(())
    }
}

/// Individual account state managed as PDAs
#[account]
#[derive(InitSpace)]
pub struct AccountState {
    /// Client ID that created this account
    #[max_len(MAX_CLIENT_ID_LENGTH)]
    pub client_id: String,

    /// Original sender (checksummed hex address from source chain)
    #[max_len(MAX_SENDER_LENGTH)]
    pub sender: String,

    /// Salt for unique account generation
    #[max_len(MAX_SALT_LENGTH)]
    pub salt: Vec<u8>,

    /// Execution nonce for replay protection
    pub nonce: u64,

    /// Account creation timestamp
    pub created_at: i64,

    /// Last execution timestamp
    pub last_executed_at: i64,

    /// Total successful executions
    pub execution_count: u64,

    /// PDA bump seed
    pub bump: u8,
}

impl AccountState {
    pub const SEED: &'static [u8] = solana_ibc_types::GmpAccountState::SEED;

    /// Derive PDA address for an account
    /// Note: If sender is >32 bytes, it will be hashed to fit Solana's PDA seed constraints
    pub fn derive_address(
        client_id: &str,
        sender: &str,
        salt: &[u8],
        program_id: &Pubkey,
    ) -> Result<(Pubkey, u8)> {
        use solana_program::hash::hash;

        require!(
            client_id.len() <= MAX_CLIENT_ID_LENGTH,
            GMPError::ClientIdTooLong
        );
        require!(sender.len() <= MAX_SENDER_LENGTH, GMPError::SenderTooLong);
        require!(salt.len() <= MAX_SALT_LENGTH, GMPError::SaltTooLong);

        // Always hash the sender to ensure consistent PDA derivation regardless of address format
        // This makes the derivation deterministic and supports any address length
        let sender_hash = hash(sender.as_bytes()).to_bytes();

        let (address, bump) = Pubkey::find_program_address(
            &[Self::SEED, client_id.as_bytes(), &sender_hash, salt],
            program_id,
        );

        Ok((address, bump))
    }

    /// Get signer seeds for CPI calls
    /// Note: Sender is always hashed to match PDA derivation
    pub fn signer_seeds(&self) -> Vec<Vec<u8>> {
        use solana_program::hash::hash;

        let sender_hash = hash(self.sender.as_bytes()).to_bytes();

        vec![
            Self::SEED.to_vec(),
            self.client_id.as_bytes().to_vec(),
            sender_hash.to_vec(),
            self.salt.clone(),
            vec![self.bump],
        ]
    }

    /// Increment nonce and update execution stats
    #[allow(clippy::missing_const_for_fn)]
    pub fn execute_nonce_increment(&mut self, current_time: i64) {
        self.nonce = self.nonce.saturating_add(1);
        self.last_executed_at = current_time;
        self.execution_count = self.execution_count.saturating_add(1);
    }
}

/// GMP packet data structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct GMPPacketData {
    /// Client ID for account derivation
    pub client_id: String,

    /// Original sender address (hex string)
    pub sender: String,

    /// Target receiver address
    /// - For incoming packets (Cosmos → Solana): Solana Pubkey as base58 string
    /// - For outgoing packets (Solana → Cosmos): Cosmos address (bech32) or empty string
    pub receiver: String,

    /// Salt for account uniqueness
    pub salt: Vec<u8>,

    /// Serialized execution payload
    pub payload: Vec<u8>,

    /// Optional memo field
    pub memo: String,
}

impl GMPPacketData {
    /// Validate packet data
    pub fn validate(&self) -> Result<()> {
        require!(!self.client_id.is_empty(), GMPError::InvalidPacketData);
        require!(
            self.client_id.len() <= MAX_CLIENT_ID_LENGTH,
            GMPError::ClientIdTooLong
        );

        require!(!self.sender.is_empty(), GMPError::InvalidPacketData);
        require!(
            self.sender.len() <= MAX_SENDER_LENGTH,
            GMPError::SenderTooLong
        );

        require!(self.salt.len() <= MAX_SALT_LENGTH, GMPError::SaltTooLong);

        require!(!self.payload.is_empty(), GMPError::EmptyPayload);
        require!(
            self.payload.len() <= MAX_PAYLOAD_LENGTH,
            GMPError::PayloadTooLong
        );

        require!(self.memo.len() <= MAX_MEMO_LENGTH, GMPError::MemoTooLong);

        Ok(())
    }
}

/// Send call message
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SendCallMsg {
    /// Source client identifier
    pub source_client: String,

    /// Timeout timestamp (unix seconds)
    pub timeout_timestamp: i64,

    /// Receiver program
    pub receiver: Pubkey,

    /// Account salt
    pub salt: Vec<u8>,

    /// Call payload (instruction data + accounts)
    pub payload: Vec<u8>,

    /// Optional memo
    pub memo: String,
}

impl SendCallMsg {
    /// Validate send call message
    pub fn validate(&self, current_time: i64) -> Result<()> {
        require!(!self.source_client.is_empty(), GMPError::InvalidPacketData);
        require!(
            self.source_client.len() <= MAX_CLIENT_ID_LENGTH,
            GMPError::ClientIdTooLong
        );

        require!(self.salt.len() <= MAX_SALT_LENGTH, GMPError::SaltTooLong);

        require!(!self.payload.is_empty(), GMPError::EmptyPayload);
        require!(
            self.payload.len() <= MAX_PAYLOAD_LENGTH,
            GMPError::PayloadTooLong
        );

        require!(self.memo.len() <= MAX_MEMO_LENGTH, GMPError::MemoTooLong);

        // Log timeout validation details
        let min_required_timeout = current_time.saturating_add(MIN_TIMEOUT_DURATION);
        let max_allowed_timeout = current_time.saturating_add(MAX_TIMEOUT_DURATION);
        msg!(
            "Timeout validation: current_time={}, timeout_timestamp={}, min_required={} (current+{}), max_allowed={} (current+{})",
            current_time,
            self.timeout_timestamp,
            min_required_timeout,
            MIN_TIMEOUT_DURATION,
            max_allowed_timeout,
            MAX_TIMEOUT_DURATION
        );

        require!(
            self.timeout_timestamp > current_time + MIN_TIMEOUT_DURATION,
            GMPError::TimeoutTooSoon
        );
        require!(
            self.timeout_timestamp < current_time + MAX_TIMEOUT_DURATION,
            GMPError::TimeoutTooLong
        );

        Ok(())
    }
}

// Re-export generated Protobuf types
pub use crate::proto::{
    GmpAcknowledgement as GMPAcknowledgement, SolanaAccountMeta, SolanaInstruction,
};

/// Helper methods for `SolanaInstruction`
impl SolanaInstruction {
    /// Parse Solana instruction from Protobuf-encoded bytes
    pub fn try_from_slice(data: &[u8]) -> Result<Self> {
        use prost::Message;
        Self::decode(data).map_err(|_| GMPError::InvalidExecutionPayload.into())
    }

    /// Validate Solana instruction fields
    pub fn validate(&self) -> Result<()> {
        require!(self.program_id.len() == 32, GMPError::InvalidProgramId);
        require!(!self.data.is_empty(), GMPError::EmptyPayload);
        require!(self.accounts.len() <= 32, GMPError::TooManyAccounts);

        // Validate all account pubkeys are 32 bytes
        for account in &self.accounts {
            require!(account.pubkey.len() == 32, GMPError::InvalidAccountKey);
        }

        Ok(())
    }

    /// Convert Protobuf account metas to Anchor `AccountMeta` format
    pub fn to_account_metas(&self) -> Result<Vec<AccountMeta>> {
        let mut account_metas = Vec::new();
        for meta in &self.accounts {
            let pubkey = Pubkey::try_from(meta.pubkey.as_slice())
                .map_err(|_| GMPError::InvalidAccountKey)?;

            // Use is_signer directly from the protobuf
            // This indicates whether the account should sign at CPI instruction level
            account_metas.push(AccountMeta {
                pubkey,
                is_signer: meta.is_signer,
                is_writable: meta.is_writable,
            });
        }
        Ok(account_metas)
    }

    /// Extract program ID as Solana Pubkey
    pub fn get_program_id(&self) -> Result<Pubkey> {
        Pubkey::try_from(self.program_id.as_slice()).map_err(|_| GMPError::InvalidProgramId.into())
    }
}

/// Helper methods for `GMPAcknowledgement`
impl GMPAcknowledgement {
    /// Create success acknowledgement
    pub const fn success(data: Vec<u8>) -> Self {
        Self {
            success: true,
            data,
            error: String::new(), // Proto uses empty string instead of Option
        }
    }

    /// Create error acknowledgement
    pub const fn error(message: String) -> Self {
        Self {
            success: false,
            data: Vec::new(),
            error: message,
        }
    }

    /// Serialize to Protobuf bytes (compatible with Borsh `try_to_vec`)
    pub fn try_to_vec(&self) -> Result<Vec<u8>> {
        use prost::Message;
        let mut buf = Vec::new();
        self.encode(&mut buf)
            .map_err(|_| GMPError::InvalidExecutionPayload)?;
        Ok(buf)
    }

    /// Deserialize from Protobuf bytes (compatible with Borsh `try_from_slice`)
    pub fn try_from_slice(data: &[u8]) -> Result<Self> {
        use prost::Message;
        Self::decode(data).map_err(|_| GMPError::InvalidExecutionPayload.into())
    }
}
