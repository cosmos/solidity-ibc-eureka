//! IFT (Inter-chain Fungible Token) callback account extraction utilities
//!
//! This module handles extraction of IFT callback accounts for acknowledgement packets.
//! When GMP packets originate from IFT (sender is IFT program), the relayer needs to
//! include IFT's on_ack_packet accounts so GMP can forward the acknowledgement.

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

/// Extract IFT callback accounts for an ack packet
///
/// When a GMP packet's sender is an IFT program, we need to include additional
/// accounts so GMP can forward the acknowledgement to IFT for refund processing.
///
/// # Arguments
/// * `source_port` - The packet source port
/// * `encoding` - The payload encoding type
/// * `payload_value` - The raw payload data
/// * `source_client` - The source client ID
/// * `sequence` - The packet sequence number
/// * `solana_client` - RPC client for Solana queries
/// * `router_program_id` - ICS26 router program ID
/// * `fee_payer` - Transaction fee payer
///
/// # Returns
/// Vector of IFT callback accounts, or empty vector if not an IFT packet
pub async fn extract_ift_ack_callback_accounts(
    source_port: &str,
    encoding: &str,
    payload_value: &[u8],
    source_client: &str,
    sequence: u64,
    solana_client: &Arc<RpcClient>,
    router_program_id: Pubkey,
    fee_payer: Pubkey,
) -> Result<Vec<AccountMeta>> {
    tracing::info!(
        "IFT: extract_ift_ack_callback_accounts called - port={}, encoding={}, client={}, seq={}",
        source_port,
        encoding,
        source_client,
        sequence
    );

    // Only process GMP port packets with protobuf encoding
    if source_port != GMP_PORT_ID || encoding != PROTOBUF_ENCODING {
        tracing::info!(
            "IFT: Skipping - not a GMP packet (port={} vs {}, encoding={} vs {})",
            source_port,
            GMP_PORT_ID,
            encoding,
            PROTOBUF_ENCODING
        );
        return Ok(Vec::new());
    }

    tracing::info!("IFT: Port and encoding match GMP, decoding packet...");

    // Decode GMP packet to get sender
    let gmp_packet = match GmpPacketData::decode_vec(payload_value) {
        Ok(packet) => packet,
        Err(e) => {
            tracing::warn!("IFT: Failed to decode GMP packet: {e:?}");
            return Ok(Vec::new());
        }
    };

    tracing::info!("IFT: GMP packet decoded, sender={}", gmp_packet.sender);

    // Parse sender as Pubkey (potential IFT program ID)
    let sender_program = match Pubkey::from_str(&gmp_packet.sender) {
        Ok(pk) => pk,
        Err(e) => {
            tracing::warn!("IFT: GMP sender is not a valid Pubkey: {e:?}");
            return Ok(Vec::new());
        }
    };

    tracing::info!(
        "IFT: Checking if GMP sender {} is an IFT program for sequence {} on client {}",
        sender_program,
        sequence,
        source_client
    );

    // Try to find pending transfer for this (client_id, sequence)
    tracing::info!("IFT: Searching for pending transfer...");
    let pending_transfer =
        match find_pending_transfer(solana_client, sender_program, source_client, sequence).await {
            Ok(Some(pt)) => {
                tracing::info!("IFT: Found pending transfer!");
                pt
            }
            Ok(None) => {
                tracing::info!(
                    "IFT: No pending transfer found for program {}, client={}, seq={}",
                    sender_program,
                    source_client,
                    sequence
                );
                return Ok(Vec::new());
            }
            Err(e) => {
                tracing::error!("IFT: Error searching for pending transfer: {e:?}");
                return Ok(Vec::new());
            }
        };

    tracing::info!(
        "Found IFT pending transfer: mint={}, sender={}, amount={}",
        pending_transfer.mint,
        pending_transfer.sender,
        pending_transfer.amount
    );

    // Build callback accounts
    build_ift_ack_accounts(
        solana_client,
        sender_program,
        &pending_transfer,
        source_client,
        sequence,
        router_program_id,
        fee_payer,
    )
}

/// Find pending transfer by client_id and sequence
async fn find_pending_transfer(
    solana_client: &Arc<RpcClient>,
    ift_program_id: Pubkey,
    client_id: &str,
    sequence: u64,
) -> Result<Option<PendingTransfer>> {
    // Query all IFT program accounts without filtering (RPC filter encoding has compatibility issues)
    // Then filter locally by discriminator
    //
    // NOTE: We use `get_program_accounts` (no config) to avoid any filter encoding issues
    // with the test validator. The validator version mismatch causes base58/base64 encoding
    // errors even when we specify no filters.
    tracing::info!(
        "IFT: Querying all accounts for program {} (no RPC filters, filtering locally)",
        ift_program_id
    );

    let all_accounts = solana_client
        .get_program_accounts(&ift_program_id)
        .map_err(|e| anyhow::anyhow!("Failed to get program accounts: {e}"))?;

    // Filter accounts by discriminator locally
    let accounts: Vec<_> = all_accounts
        .into_iter()
        .filter(|(_, account)| {
            account.data.len() >= ANCHOR_DISCRIMINATOR_SIZE
                && &account.data[..ANCHOR_DISCRIMINATOR_SIZE] == &*PENDING_TRANSFER_DISCRIMINATOR
        })
        .collect();

    tracing::info!("IFT: Found {} pending transfer accounts", accounts.len());

    for (pubkey, account) in accounts {
        if account.data.len() < ANCHOR_DISCRIMINATOR_SIZE {
            tracing::debug!("IFT: Skipping account {} - data too short", pubkey);
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

        tracing::info!(
            "IFT: Checking pending transfer at {}: client_id={}, sequence={} (looking for client={}, seq={})",
            pubkey,
            pending.client_id,
            pending.sequence,
            client_id,
            sequence
        );

        // Match by client_id and sequence
        if pending.client_id == client_id && pending.sequence == sequence {
            tracing::info!(
                "IFT: MATCH! Found pending transfer at {}: client_id={}, sequence={}",
                pubkey,
                pending.client_id,
                pending.sequence
            );
            return Ok(Some(pending));
        }
    }

    tracing::info!("IFT: No matching pending transfer found after checking all accounts");
    Ok(None)
}

/// Build IFT on_ack_packet callback accounts
fn build_ift_ack_accounts(
    _solana_client: &Arc<RpcClient>,
    ift_program_id: Pubkey,
    pending_transfer: &PendingTransfer,
    client_id: &str,
    sequence: u64,
    router_program_id: Pubkey,
    fee_payer: Pubkey,
) -> Result<Vec<AccountMeta>> {
    let mint = pending_transfer.mint;

    // Derive IFT app state PDA
    let (app_state_pda, _) =
        Pubkey::find_program_address(&[IFT_APP_STATE_SEED, mint.as_ref()], &ift_program_id);

    // Derive pending transfer PDA
    let (pending_transfer_pda, _) = Pubkey::find_program_address(
        &[
            PENDING_TRANSFER_SEED,
            mint.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &ift_program_id,
    );

    // Derive mint authority PDA
    let (mint_authority_pda, _) =
        Pubkey::find_program_address(&[MINT_AUTHORITY_SEED, mint.as_ref()], &ift_program_id);

    // Derive sender's token account (Associated Token Account)
    let sender_token_account = get_associated_token_address(&pending_transfer.sender, &mint);

    // Log derived accounts for debugging
    tracing::info!(
        "IFT account derivation: app_state={}, pending_transfer={}, mint={}, mint_authority={}, sender_ata={}",
        app_state_pda,
        pending_transfer_pda,
        mint,
        mint_authority_pda,
        sender_token_account
    );

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
        // app_state
        AccountMeta {
            pubkey: app_state_pda,
            is_signer: false,
            is_writable: true,
        },
        // pending_transfer
        AccountMeta {
            pubkey: pending_transfer_pda,
            is_signer: false,
            is_writable: true,
        },
        // mint
        AccountMeta {
            pubkey: mint,
            is_signer: false,
            is_writable: true,
        },
        // mint_authority
        AccountMeta {
            pubkey: mint_authority_pda,
            is_signer: false,
            is_writable: false,
        },
        // sender_token_account
        AccountMeta {
            pubkey: sender_token_account,
            is_signer: false,
            is_writable: true,
        },
        // router_program
        AccountMeta {
            pubkey: router_program_id,
            is_signer: false,
            is_writable: false,
        },
        // instruction_sysvar
        AccountMeta {
            pubkey: solana_sdk::sysvar::instructions::id(),
            is_signer: false,
            is_writable: false,
        },
        // payer
        AccountMeta {
            pubkey: fee_payer,
            is_signer: true,
            is_writable: true,
        },
        // token_program
        AccountMeta {
            pubkey: spl_token::id(),
            is_signer: false,
            is_writable: false,
        },
        // system_program
        AccountMeta {
            pubkey: solana_sdk::system_program::id(),
            is_signer: false,
            is_writable: false,
        },
    ];

    tracing::info!(
        "Built {} IFT callback accounts for ack packet (mint: {})",
        accounts.len(),
        mint
    );

    Ok(accounts)
}
