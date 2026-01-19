use crate::constants::*;
use anchor_lang::prelude::*;
use solana_sha256_hasher::hash;

/// Account schema version
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug)]
pub enum AccountVersion {
    V1,
}

/// Main GMP application state
#[account]
#[derive(InitSpace)]
pub struct GMPAppState {
    /// Schema version for upgrades
    pub version: AccountVersion,

    /// Emergency pause flag
    pub paused: bool,

    /// PDA bump seed
    pub bump: u8,

    /// Access manager program ID for role-based access control
    pub access_manager: Pubkey,

    /// Reserved space for future fields
    pub _reserved: [u8; 256],
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
}

/// Send call message (unvalidated input from user)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SendCallMsg {
    /// Source client identifier
    pub source_client: String,

    /// Timeout timestamp (unix seconds)
    pub timeout_timestamp: i64,

    /// Receiver address (string format to support any destination chain)
    pub receiver: String,

    /// Account salt
    pub salt: Vec<u8>,

    /// Call payload (instruction data + accounts)
    pub payload: Vec<u8>,

    /// Optional memo
    pub memo: String,
}

// Re-export types from proto crate
pub use crate::proto::{
    GmpAcknowledgement, GmpSolanaPayload, GmpValidationError, SolanaAccountMeta,
};

pub use solana_ibc_types::{CallResultStatus, GMPCallResult};

/// Stores result of a GMP call (ack or timeout) for sender queries
#[account]
#[derive(InitSpace)]
pub struct GMPCallResultAccount {
    pub version: AccountVersion,
    #[max_len(128)]
    pub sender: String,
    pub sequence: u64,
    #[max_len(64)]
    pub source_client: String,
    #[max_len(64)]
    pub dest_client: String,
    pub status: CallResultStatus,
    /// SHA256 commitment of the acknowledgement
    pub ack_commitment: [u8; 32],
    pub result_timestamp: i64,
    pub bump: u8,
}

impl GMPCallResultAccount {
    /// Initialize the account with acknowledgement data.
    pub fn init_acknowledged(
        &mut self,
        msg: solana_ibc_types::OnAcknowledgementPacketMsg,
        sender: String,
        timestamp: i64,
        bump: u8,
    ) {
        self.version = AccountVersion::V1;
        self.sender = sender;
        self.sequence = msg.sequence;
        self.source_client = msg.source_client;
        self.dest_client = msg.dest_client;
        self.status = CallResultStatus::Acknowledgement;
        self.ack_commitment = hash(&msg.acknowledgement).to_bytes();
        self.result_timestamp = timestamp;
        self.bump = bump;
    }

    /// Initialize the account with timeout data.
    pub fn init_timed_out(
        &mut self,
        msg: solana_ibc_types::OnTimeoutPacketMsg,
        sender: String,
        timestamp: i64,
        bump: u8,
    ) {
        self.version = AccountVersion::V1;
        self.sender = sender;
        self.sequence = msg.sequence;
        self.source_client = msg.source_client;
        self.dest_client = msg.dest_client;
        self.status = CallResultStatus::Timeout;
        self.ack_commitment = [0u8; 32];
        self.result_timestamp = timestamp;
        self.bump = bump;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_ibc_types::Payload;

    fn test_payload() -> Payload {
        Payload {
            source_port: "gmpport".to_string(),
            dest_port: "gmpport".to_string(),
            version: "ics27-2".to_string(),
            encoding: "application/x-solidity-abi".to_string(),
            value: vec![],
        }
    }

    #[test]
    fn test_ack_commitment_computation() {
        let mut account = GMPCallResultAccount {
            version: AccountVersion::V1,
            sender: String::new(),
            sequence: 0,
            source_client: String::new(),
            dest_client: String::new(),
            status: CallResultStatus::Acknowledgement,
            ack_commitment: [0u8; 32],
            result_timestamp: 0,
            bump: 0,
        };

        let ack_data = b"test acknowledgement data";
        let msg = solana_ibc_types::OnAcknowledgementPacketMsg {
            sequence: 42,
            source_client: "source-client".to_string(),
            dest_client: "dest-client".to_string(),
            payload: test_payload(),
            acknowledgement: ack_data.to_vec(),
            relayer: Pubkey::default(),
        };

        account.init_acknowledged(msg, "sender".to_string(), 1234567890, 255);

        let expected_commitment = hash(ack_data).to_bytes();
        assert_eq!(account.ack_commitment, expected_commitment);
        assert_ne!(account.ack_commitment, [0u8; 32]);
        assert_eq!(account.status, CallResultStatus::Acknowledgement);
        assert_eq!(account.sequence, 42);
    }

    #[test]
    fn test_timeout_has_zero_commitment() {
        let mut account = GMPCallResultAccount {
            version: AccountVersion::V1,
            sender: String::new(),
            sequence: 0,
            source_client: String::new(),
            dest_client: String::new(),
            status: CallResultStatus::Acknowledgement,
            ack_commitment: [0u8; 32],
            result_timestamp: 0,
            bump: 0,
        };

        let msg = solana_ibc_types::OnTimeoutPacketMsg {
            sequence: 42,
            source_client: "source-client".to_string(),
            dest_client: "dest-client".to_string(),
            payload: test_payload(),
            relayer: Pubkey::default(),
        };

        account.init_timed_out(msg, "sender".to_string(), 1234567890, 255);

        assert_eq!(account.ack_commitment, [0u8; 32]);
        assert_eq!(account.status, CallResultStatus::Timeout);
        assert_eq!(account.sequence, 42);
    }
}
