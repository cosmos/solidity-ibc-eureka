use anchor_lang::prelude::*;

use crate::{constants::*, errors::IFTError};

/// Account schema version for upgrades
#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug, Default,
)]
pub enum AccountVersion {
    #[default]
    V1,
}

/// Chain-specific options for counterparty chain configuration
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, InitSpace)]
pub enum ChainOptions {
    /// EVM chain - encode as ABI call to iftMint(address, uint256)
    Evm,
    /// Cosmos chain - encode as protojson `MsgIFTMint`
    Cosmos {
        /// Token denom on counterparty chain (Cosmos SDK max: 128 chars)
        #[max_len(128)]
        denom: String,
        /// Protobuf type URL for `MsgIFTMint` (e.g., "/cosmos.ift.v1.MsgIFTMint")
        #[max_len(128)]
        type_url: String,
        /// ICS27-GMP interchain account address (the signer for `MsgIFTMint`)
        #[max_len(128)]
        ica_address: String,
    },
}

impl ChainOptions {
    /// Validate Chain Options params
    pub fn validate(&self) -> Result<()> {
        if let Self::Cosmos {
            ref denom,
            ref type_url,
            ref ica_address,
        } = self
        {
            require!(!denom.is_empty(), IFTError::CosmosEmptyCounterpartyDenom);
            require!(!type_url.is_empty(), IFTError::CosmosEmptyTypeUrl);
            require!(!ica_address.is_empty(), IFTError::CosmosEmptyIcaAddress);
            require!(
                denom.len() <= MAX_COUNTERPARTY_ADDRESS_LENGTH,
                IFTError::InvalidCounterpartyDenomLength
            );
            require!(
                type_url.len() <= MAX_COUNTERPARTY_ADDRESS_LENGTH,
                IFTError::InvalidCosmosTypeUrlLength
            );
            require!(
                ica_address.len() <= MAX_COUNTERPARTY_ADDRESS_LENGTH,
                IFTError::InvalidCosmosIcaAddressLength
            );
        }

        Ok(())
    }
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

    /// Mint authority PDA bump for signing mint/refund operations.
    /// Stored to use `create_program_address` (~1.5k CUs) instead of
    /// `find_program_address` (~10k CUs) on each mint/refund.
    pub mint_authority_bump: u8,

    /// Admin authority for this IFT token
    pub admin: Pubkey,

    /// GMP program address for sending cross-chain calls
    pub gmp_program: Pubkey,

    /// Daily mint rate limit (0 = no limit)
    pub daily_mint_limit: u64,
    /// Current rate limit day (`unix_timestamp` / `SECONDS_PER_DAY`)
    pub rate_limit_day: u64,
    /// Net mint usage for the current day
    pub rate_limit_daily_usage: u64,

    /// Whether this token is paused (blocks mint and transfer, not refunds)
    pub paused: bool,

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

    /// IBC client identifier for this bridge
    #[max_len(64)]
    pub client_id: String,

    /// IFT contract address on counterparty chain (EVM address or Cosmos bech32)
    #[max_len(128)]
    pub counterparty_ift_address: String,

    /// Chain-specific options for constructing mint calls
    pub chain_options: ChainOptions,

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
    /// Chain-specific options
    pub chain_options: ChainOptions,
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
}

/// Message for setting the daily mint rate limit
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SetMintRateLimitMsg {
    /// Daily mint limit (0 = no limit)
    pub daily_mint_limit: u64,
}

/// Message for pausing/unpausing an IFT token
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SetPausedMsg {
    /// Whether to pause (true) or unpause (false) the token
    pub paused: bool,
}

#[cfg(test)]
mod tests;
