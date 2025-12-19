use anchor_lang::prelude::*;

use crate::state::CounterpartyChainType;

/// Event emitted when IFT app is initialized
#[event]
pub struct IFTAppInitialized {
    /// SPL Token mint address
    pub mint: Pubkey,
    /// Token decimals
    pub decimals: u8,
    /// Access manager program
    pub access_manager: Pubkey,
    /// GMP program for cross-chain calls
    pub gmp_program: Pubkey,
    /// Initialization timestamp
    pub timestamp: i64,
}

/// Event emitted when an IFT bridge is registered
#[event]
pub struct IFTBridgeRegistered {
    /// SPL Token mint address
    pub mint: Pubkey,
    /// IBC client identifier
    pub client_id: String,
    /// Counterparty IFT contract address
    pub counterparty_ift_address: String,
    /// Counterparty chain type
    pub counterparty_chain_type: CounterpartyChainType,
    /// Registration timestamp
    pub timestamp: i64,
}

/// Event emitted when an IFT bridge is removed
#[event]
pub struct IFTBridgeRemoved {
    /// SPL Token mint address
    pub mint: Pubkey,
    /// IBC client identifier
    pub client_id: String,
    /// Removal timestamp
    pub timestamp: i64,
}

/// Event emitted when an IFT transfer is initiated
#[event]
pub struct IFTTransferInitiated {
    /// SPL Token mint address
    pub mint: Pubkey,
    /// IBC client identifier
    pub client_id: String,
    /// Packet sequence number
    pub sequence: u64,
    /// Sender pubkey
    pub sender: Pubkey,
    /// Receiver address on destination chain
    pub receiver: String,
    /// Amount transferred
    pub amount: u64,
    /// Timeout timestamp
    pub timeout_timestamp: i64,
}

/// Event emitted when tokens are minted from a cross-chain transfer
#[event]
pub struct IFTMintReceived {
    /// SPL Token mint address
    pub mint: Pubkey,
    /// IBC client identifier
    pub client_id: String,
    /// Receiver pubkey
    pub receiver: Pubkey,
    /// Amount minted
    pub amount: u64,
    /// GMP account that authorized the mint
    pub gmp_account: Pubkey,
    /// Mint timestamp
    pub timestamp: i64,
}

/// Event emitted when an IFT transfer is completed (acknowledged)
#[event]
pub struct IFTTransferCompleted {
    /// SPL Token mint address
    pub mint: Pubkey,
    /// IBC client identifier
    pub client_id: String,
    /// Packet sequence number
    pub sequence: u64,
    /// Original sender
    pub sender: Pubkey,
    /// Amount that was transferred
    pub amount: u64,
    /// Completion timestamp
    pub timestamp: i64,
}

/// Event emitted when an IFT transfer is refunded (failed or timed out)
#[event]
pub struct IFTTransferRefunded {
    /// SPL Token mint address
    pub mint: Pubkey,
    /// IBC client identifier
    pub client_id: String,
    /// Packet sequence number
    pub sequence: u64,
    /// Original sender (refund recipient)
    pub sender: Pubkey,
    /// Amount refunded
    pub amount: u64,
    /// Reason for refund
    pub reason: RefundReason,
    /// Refund timestamp
    pub timestamp: i64,
}

/// Reason for transfer refund
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum RefundReason {
    /// Transfer timed out
    Timeout,
    /// Transfer failed on destination
    Failed,
}

/// Event emitted when IFT app is paused
#[event]
pub struct IFTAppPaused {
    /// SPL Token mint address
    pub mint: Pubkey,
    /// Admin who paused the app
    pub admin: Pubkey,
    /// Pause timestamp
    pub timestamp: i64,
}

/// Event emitted when IFT app is unpaused
#[event]
pub struct IFTAppUnpaused {
    /// SPL Token mint address
    pub mint: Pubkey,
    /// Admin who unpaused the app
    pub admin: Pubkey,
    /// Unpause timestamp
    pub timestamp: i64,
}
