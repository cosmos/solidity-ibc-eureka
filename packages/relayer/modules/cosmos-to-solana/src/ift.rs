//! IFT (Inter-chain Fungible Token) callback account extraction utilities
//!
//! This module handles extraction of IFT callback accounts for acknowledgement and timeout packets.
//! When GMP packets originate from IFT (sender is IFT program), the relayer needs to
//! include IFT's callback accounts so GMP can forward the ack/timeout to IFT for refund processing.

use std::str::FromStr;
use std::sync::{Arc, LazyLock};

use anchor_lang::prelude::*;
use anyhow::Result;
use sha2::{Digest, Sha256};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey};
use spl_associated_token_account::get_associated_token_address;

use crate::constants::{ANCHOR_DISCRIMINATOR_SIZE, GMP_PORT_ID, PROTOBUF_ENCODING};
use crate::proto::{GmpPacketData, Protobuf};

/// IFT PDA seeds (must match ics27-ift program)
const IFT_APP_STATE_SEED: &[u8] = b"ift_app_state";
const PENDING_TRANSFER_SEED: &[u8] = b"pending_transfer";
const MINT_AUTHORITY_SEED: &[u8] = b"ift_mint_authority";

static PENDING_TRANSFER_DISCRIMINATOR: LazyLock<[u8; 8]> = LazyLock::new(|| {
    let mut hasher = Sha256::new();
    hasher.update(b"account:PendingTransfer");
    let result = hasher.finalize();
    result[..8].try_into().expect("sha256 produces 32 bytes")
});

/// Deserialized `PendingTransfer` account data
#[derive(AnchorSerialize, AnchorDeserialize, Debug)]
struct PendingTransfer {
    pub version: u8,
    pub bump: u8,
    pub mint: Pubkey,
    pub client_id: String,
    pub sequence: u64,
    pub sender: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
    pub _reserved: [u8; 32],
}

/// Parameters for extracting IFT callback accounts
pub struct IftCallbackParams<'a> {
    /// The packet source port
    pub source_port: &'a str,
    /// The payload encoding type
    pub encoding: &'a str,
    /// The raw payload data
    pub payload_value: &'a [u8],
    /// The source client ID
    pub source_client: &'a str,
    /// The packet sequence number
    pub sequence: u64,
    /// RPC client for Solana queries
    pub solana_client: &'a Arc<RpcClient>,
    /// ICS26 router program ID
    pub router_program_id: Pubkey,
    /// Transaction fee payer
    pub fee_payer: Pubkey,
}

/// Extract IFT callback accounts for ack/timeout packets
///
/// When a GMP packet's sender is an IFT program, we need to include additional
/// accounts so GMP can forward the ack/timeout to IFT for refund processing.
///
/// Returns empty vec if the packet is not from IFT or pending transfer not found.
#[must_use]
pub fn extract_ift_callback_accounts(params: &IftCallbackParams<'_>) -> Vec<AccountMeta> {
    // Only process GMP port packets with protobuf encoding
    if params.source_port != GMP_PORT_ID || params.encoding != PROTOBUF_ENCODING {
        return Vec::new();
    }

    let gmp_packet = match GmpPacketData::decode_vec(params.payload_value) {
        Ok(packet) => packet,
        Err(e) => {
            tracing::warn!("IFT: Failed to decode GMP packet: {e:?}");
            return Vec::new();
        }
    };

    // Parse sender as Pubkey (potential IFT program ID)
    let sender_program = match Pubkey::from_str(&gmp_packet.sender) {
        Ok(pk) => pk,
        Err(e) => {
            tracing::warn!("IFT: GMP sender is not a valid Pubkey: {e:?}");
            return Vec::new();
        }
    };

    // Try to find pending transfer for this (client_id, sequence)
    let pending_transfer = match find_pending_transfer(
        params.solana_client,
        sender_program,
        params.source_client,
        params.sequence,
    ) {
        Ok(Some(pt)) => pt,
        Ok(None) => {
            return Vec::new();
        }
        Err(e) => {
            tracing::error!("IFT: Error searching for pending transfer: {e:?}");
            return Vec::new();
        }
    };

    tracing::debug!(
        "Found IFT pending transfer: mint={}, sender={}, amount={}",
        pending_transfer.mint,
        pending_transfer.sender,
        pending_transfer.amount
    );

    build_ift_callback_accounts(
        sender_program,
        &pending_transfer,
        params.source_client,
        params.sequence,
        params.router_program_id,
        params.fee_payer,
    )
}

/// Find pending transfer by `client_id` and sequence
fn find_pending_transfer(
    solana_client: &Arc<RpcClient>,
    ift_program_id: Pubkey,
    client_id: &str,
    sequence: u64,
) -> Result<Option<PendingTransfer>> {
    // Query all IFT program accounts without filtering (RPC filter encoding has compatibility issues)
    // Then filter locally by discriminator
    let all_accounts = solana_client
        .get_program_accounts(&ift_program_id)
        .map_err(|e| anyhow::anyhow!("Failed to get program accounts: {e}"))?;

    // Filter accounts by discriminator locally
    let accounts: Vec<_> = all_accounts
        .into_iter()
        .filter(|(_, account)| {
            account.data.len() >= ANCHOR_DISCRIMINATOR_SIZE
                && account.data[..ANCHOR_DISCRIMINATOR_SIZE] == *PENDING_TRANSFER_DISCRIMINATOR
        })
        .collect();

    for (pubkey, account) in accounts {
        if account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            continue;
        }

        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let pending = match PendingTransfer::deserialize(&mut data) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("IFT: Failed to deserialize account {}: {e:?}", pubkey);
                continue;
            }
        };

        if pending.client_id == client_id && pending.sequence == sequence {
            return Ok(Some(pending));
        }
    }

    Ok(None)
}

/// Build IFT callback accounts (same for both `on_ack_packet` and `on_timeout_packet`)
fn build_ift_callback_accounts(
    ift_program_id: Pubkey,
    pending_transfer: &PendingTransfer,
    client_id: &str,
    sequence: u64,
    router_program_id: Pubkey,
    fee_payer: Pubkey,
) -> Vec<AccountMeta> {
    let mint = pending_transfer.mint;

    let (app_state_pda, _) =
        Pubkey::find_program_address(&[IFT_APP_STATE_SEED, mint.as_ref()], &ift_program_id);

    let (pending_transfer_pda, _) = Pubkey::find_program_address(
        &[
            PENDING_TRANSFER_SEED,
            mint.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &ift_program_id,
    );

    let (mint_authority_pda, _) =
        Pubkey::find_program_address(&[MINT_AUTHORITY_SEED, mint.as_ref()], &ift_program_id);

    // Derive sender's token account (Associated Token Account)
    let sender_token_account = get_associated_token_address(&pending_transfer.sender, &mint);

    // Build account list matching IFT's OnAckPacket struct order
    // Note: IFT program ID MUST be included because Solana's invoke() requires
    // the target program to be in the account_infos slice
    let accounts = vec![
        // IFT program (required for CPI from GMP)
        AccountMeta {
            pubkey: ift_program_id,
            is_signer: false,
            is_writable: false,
        },
        AccountMeta {
            pubkey: app_state_pda,
            is_signer: false,
            is_writable: true,
        },
        AccountMeta {
            pubkey: pending_transfer_pda,
            is_signer: false,
            is_writable: true,
        },
        AccountMeta {
            pubkey: mint,
            is_signer: false,
            is_writable: true,
        },
        AccountMeta {
            pubkey: mint_authority_pda,
            is_signer: false,
            is_writable: false,
        },
        AccountMeta {
            pubkey: sender_token_account,
            is_signer: false,
            is_writable: true,
        },
        AccountMeta {
            pubkey: router_program_id,
            is_signer: false,
            is_writable: false,
        },
        AccountMeta {
            pubkey: solana_sdk::sysvar::instructions::id(),
            is_signer: false,
            is_writable: false,
        },
        AccountMeta {
            pubkey: fee_payer,
            is_signer: true,
            is_writable: true,
        },
        AccountMeta {
            pubkey: spl_token::id(),
            is_signer: false,
            is_writable: false,
        },
        AccountMeta {
            pubkey: solana_sdk::system_program::id(),
            is_signer: false,
            is_writable: false,
        },
    ];

    accounts
}
