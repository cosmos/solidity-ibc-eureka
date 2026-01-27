use anchor_lang::prelude::*;

use crate::constants::*;

/// Account schema version for upgrades
#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug, Default,
)]
pub enum AccountVersion {
    #[default]
    V1,
}

/// Counterparty chain type for constructing mint calls
#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug, Default,
)]
pub enum CounterpartyChainType {
    /// EVM chain - encode as ABI call to iftMint(address, uint256)
    #[default]
    Evm,
    /// Cosmos chain - encode as protojson `MsgIFTMint`
    Cosmos,
    /// Solana chain - encode as Solana instruction data
    Solana,
}

/// Main IFT application state
/// PDA Seeds: [`IFT_APP_STATE_SEED`, `mint.as_ref()`]
#[account]
#[derive(InitSpace)]
pub struct IFTAppState {
    pub version: AccountVersion,
    pub bump: u8,

    /// SPL Token mint address (this IFT controls this mint)
    pub mint: Pubkey,

    /// Mint authority PDA bump (for signing mint operations)
    pub mint_authority_bump: u8,

    /// Access manager program ID for role-based access control
    pub access_manager: Pubkey,

    /// GMP program address for sending cross-chain calls
    pub gmp_program: Pubkey,

    pub _reserved: [u8; 128],
}

impl IFTAppState {
    /// Get PDA seeds for app state
    pub fn seeds(mint: &Pubkey) -> Vec<Vec<u8>> {
        vec![IFT_APP_STATE_SEED.to_vec(), mint.as_ref().to_vec()]
    }

    /// Get signer seeds for this app state
    pub fn signer_seeds(&self) -> Vec<Vec<u8>> {
        vec![
            IFT_APP_STATE_SEED.to_vec(),
            self.mint.as_ref().to_vec(),
            vec![self.bump],
        ]
    }
}

/// IFT Bridge configuration for a counterparty chain
#[account]
#[derive(InitSpace)]
pub struct IFTBridge {
    pub version: AccountVersion,
    pub bump: u8,

    /// Mint this bridge is associated with
    pub mint: Pubkey,

    /// IBC client identifier on local chain
    // TODO: Remove - redundant since client_id is already the PDA seed key.
    // Pass via instruction message instead (requires adding arg to RemoveIFTBridge).
    #[max_len(64)]
    pub client_id: String,

    /// IFT contract address on counterparty chain (EVM address or Cosmos bech32)
    #[max_len(128)]
    pub counterparty_ift_address: String,

    /// Token denom on counterparty chain (Cosmos SDK max: 128 chars)
    /// For EVM chains, this can be empty as the address is used directly
    #[max_len(128)]
    pub counterparty_denom: String,

    /// Protobuf type URL for `MsgIFTMint` on Cosmos chains (e.g., "/cosmos.ift.v1.MsgIFTMint")
    /// For non-Cosmos chains, this can be empty
    #[max_len(128)]
    pub cosmos_type_url: String,

    /// ICS27-GMP interchain account address on Cosmos chain (the signer for `MsgIFTMint`)
    /// Required for Cosmos chains, empty for EVM/Solana
    #[max_len(128)]
    pub cosmos_ica_address: String,

    /// Counterparty chain type (for call constructor logic)
    pub counterparty_chain_type: CounterpartyChainType,

    /// Whether bridge is active
    pub active: bool,

    pub _reserved: [u8; 64],
}

impl IFTBridge {
    pub fn seeds(mint: &Pubkey, client_id: &str) -> Vec<Vec<u8>> {
        vec![
            IFT_BRIDGE_SEED.to_vec(),
            mint.as_ref().to_vec(),
            client_id.as_bytes().to_vec(),
        ]
    }
}

/// Pending transfer tracking for ack/timeout handling
#[account]
#[derive(InitSpace)]
pub struct PendingTransfer {
    pub version: AccountVersion,
    pub bump: u8,

    /// Mint this transfer is for
    pub mint: Pubkey,

    /// Client ID the transfer was sent to
    #[max_len(64)]
    pub client_id: String,

    /// Packet sequence number
    pub sequence: u64,

    /// Original sender (for refunds)
    pub sender: Pubkey,

    /// Amount transferred (for refunds)
    pub amount: u64,

    /// Transfer initiation timestamp
    pub timestamp: i64,

    pub _reserved: [u8; 32],
}

impl PendingTransfer {
    pub fn seeds(mint: &Pubkey, client_id: &str, sequence: u64) -> Vec<Vec<u8>> {
        vec![
            PENDING_TRANSFER_SEED.to_vec(),
            mint.as_ref().to_vec(),
            client_id.as_bytes().to_vec(),
            sequence.to_le_bytes().to_vec(),
        ]
    }
}

/// Message for registering an IFT bridge
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct RegisterIFTBridgeMsg {
    /// IBC client identifier
    pub client_id: String,
    /// Counterparty IFT contract address
    pub counterparty_ift_address: String,

    // TODO: parameters with json same as with constructor
    /// Token denom on counterparty chain (required for Cosmos, optional for EVM)
    pub counterparty_denom: String,
    /// Protobuf type URL for `MsgIFTMint` on Cosmos chains (e.g., "/cosmos.ift.v1.MsgIFTMint")
    /// Required for Cosmos chains, ignored for EVM/Solana
    pub cosmos_type_url: String,
    /// ICS27-GMP interchain account address on Cosmos chain (the signer for `MsgIFTMint`)
    /// Required for Cosmos chains, ignored for EVM/Solana
    pub cosmos_ica_address: String,
    /// Counterparty chain type
    pub counterparty_chain_type: CounterpartyChainType,
}

/// Message for initiating an IFT transfer
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct IFTTransferMsg {
    /// IBC client identifier for destination
    pub client_id: String,
    /// Receiver address on destination chain
    pub receiver: String,
    /// Amount to transfer
    pub amount: u64,
    /// Timeout timestamp (0 for default 15 minutes)
    pub timeout_timestamp: i64,
}

/// Message for minting IFT tokens (called by GMP)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct IFTMintMsg {
    /// Receiver pubkey
    pub receiver: Pubkey,
    /// Amount to mint
    pub amount: u64,
    /// IBC client identifier (for bridge lookup and GMP validation)
    pub client_id: String,
    /// GMP account PDA bump (for efficient validation with `create_program_address`)
    pub gmp_account_bump: u8,
}

#[cfg(test)]
mod tests;
