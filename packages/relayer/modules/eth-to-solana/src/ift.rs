//! IFT `claim_refund` instruction builder for ack/timeout packets.
//!
//! After GMP processes ack/timeout and creates `GMPCallResultAccount`,
//! the relayer calls IFT's `claim_refund` to process refunds.

use std::str::FromStr;
use std::sync::{Arc, LazyLock};

use anchor_lang::prelude::*;
use anyhow::Result;
use sha2::{Digest, Sha256};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use spl_associated_token_account::get_associated_token_address;

use alloy::sol_types::SolValue;

use crate::constants::ANCHOR_DISCRIMINATOR_SIZE;
use crate::gmp::{AbiGmpPacketData, ABI_ENCODING, GMP_PORT_ID, PROTOBUF_ENCODING};
use crate::proto::{GmpPacketData, Protobuf};

/// IFT PDA seeds (must match ift program)
const IFT_APP_STATE_SEED: &[u8] = b"ift_app_state";
const IFT_APP_MINT_STATE_SEED: &[u8] = b"ift_app_mint_state";
const IFT_BRIDGE_SEED: &[u8] = b"ift_bridge";
const PENDING_TRANSFER_SEED: &[u8] = b"pending_transfer";
const MINT_AUTHORITY_SEED: &[u8] = b"ift_mint_authority";

/// GMP result PDA seed (must match ics27-gmp program)
const GMP_RESULT_SEED: &[u8] = b"gmp_result";

static PENDING_TRANSFER_DISCRIMINATOR: LazyLock<[u8; 8]> = LazyLock::new(|| {
    let mut hasher = Sha256::new();
    hasher.update(b"account:PendingTransfer");
    let result = hasher.finalize();
    result[..8].try_into().expect("sha256 produces 32 bytes")
});

static FINALIZE_TRANSFER_DISCRIMINATOR: LazyLock<[u8; 8]> = LazyLock::new(|| {
    let mut hasher = Sha256::new();
    hasher.update(b"global:finalize_transfer");
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

/// Parameters for building IFT `claim_refund` instruction
pub struct ClaimRefundParams<'a> {
    pub source_port: &'a str,
    pub encoding: &'a str,
    pub payload_value: &'a [u8],
    pub source_client: &'a str,
    pub sequence: u64,
    pub solana_client: &'a Arc<RpcClient>,
    pub gmp_program_id: Pubkey,
    pub fee_payer: Pubkey,
}

/// Build IFT `claim_refund` instruction if this packet is from IFT.
/// Returns None if the packet is not an IFT transfer or no pending transfer exists.
pub fn build_claim_refund_instruction(params: &ClaimRefundParams<'_>) -> Option<Instruction> {
    // Only process GMP port packets with known encoding
    let is_protobuf = params.encoding.is_empty() || params.encoding == PROTOBUF_ENCODING;
    let is_abi = params.encoding == ABI_ENCODING;

    if params.source_port != GMP_PORT_ID || !(is_protobuf || is_abi) {
        return None;
    }

    // Decode sender from the GMP packet (protobuf or ABI)
    let sender_string = if is_abi {
        match <AbiGmpPacketData as SolValue>::abi_decode(params.payload_value) {
            Ok(packet) => packet.sender,
            Err(e) => {
                tracing::warn!(error = ?e, "IFT: Failed to ABI decode GMP packet");
                return None;
            }
        }
    } else {
        match GmpPacketData::decode_vec(params.payload_value) {
            Ok(packet) => packet.sender.to_string(),
            Err(e) => {
                tracing::warn!(error = ?e, "IFT: Failed to decode GMP packet");
                return None;
            }
        }
    };

    // Parse sender as Pubkey (the IFT program ID)
    let ift_program_id = match Pubkey::from_str(&sender_string) {
        Ok(pk) => pk,
        Err(e) => {
            tracing::warn!(error = ?e, sender = %sender_string, "IFT: GMP sender is not a valid Pubkey");
            return None;
        }
    };

    // Try to find pending transfer for this (client_id, sequence)
    let pending_transfer = match find_pending_transfer(
        params.solana_client,
        ift_program_id,
        params.source_client,
        params.sequence,
    ) {
        Ok(Some(pt)) => pt,
        Ok(None) => {
            tracing::debug!(
                client_id = params.source_client,
                sequence = params.sequence,
                "IFT: No pending transfer found"
            );
            return None;
        }
        Err(e) => {
            tracing::error!(error = ?e, "IFT: Error searching for pending transfer");
            return None;
        }
    };

    tracing::debug!(
        mint = %pending_transfer.mint,
        amount = pending_transfer.amount,
        sequence = params.sequence,
        "IFT: Building claim_refund instruction"
    );

    Some(build_finalize_transfer_ix(
        ift_program_id,
        params.gmp_program_id,
        &pending_transfer,
        params.source_client,
        params.sequence,
        params.fee_payer,
    ))
}

fn find_pending_transfer(
    solana_client: &Arc<RpcClient>,
    ift_program_id: Pubkey,
    client_id: &str,
    sequence: u64,
) -> Result<Option<PendingTransfer>> {
    let all_accounts = solana_client
        .get_program_accounts(&ift_program_id)
        .map_err(|e| anyhow::anyhow!("Failed to get program accounts: {e}"))?;

    let accounts: Vec<_> = all_accounts
        .into_iter()
        .filter(|(_, account)| {
            account.data.len() >= ANCHOR_DISCRIMINATOR_SIZE
                && account.data[..ANCHOR_DISCRIMINATOR_SIZE] == *PENDING_TRANSFER_DISCRIMINATOR
        })
        .collect();

    for (pubkey, account) in accounts {
        let mut data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let pending = match PendingTransfer::deserialize(&mut data) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!(error = ?e, account = %pubkey, "IFT: Failed to deserialize account");
                continue;
            }
        };

        if pending.client_id == client_id && pending.sequence == sequence {
            return Ok(Some(pending));
        }
    }

    Ok(None)
}

fn build_finalize_transfer_ix(
    ift_program_id: Pubkey,
    gmp_program_id: Pubkey,
    pending_transfer: &PendingTransfer,
    client_id: &str,
    sequence: u64,
    fee_payer: Pubkey,
) -> Instruction {
    let mint = pending_transfer.mint;

    // Derive PDAs
    let (app_state_pda, _) = Pubkey::find_program_address(&[IFT_APP_STATE_SEED], &ift_program_id);

    let (app_mint_state_pda, _) =
        Pubkey::find_program_address(&[IFT_APP_MINT_STATE_SEED, mint.as_ref()], &ift_program_id);

    let (ift_bridge_pda, _) = Pubkey::find_program_address(
        &[IFT_BRIDGE_SEED, mint.as_ref(), client_id.as_bytes()],
        &ift_program_id,
    );

    let (pending_transfer_pda, _) = Pubkey::find_program_address(
        &[
            PENDING_TRANSFER_SEED,
            mint.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &ift_program_id,
    );

    // GMP result PDA - owned by GMP program
    let (gmp_result_pda, _) = Pubkey::find_program_address(
        &[
            GMP_RESULT_SEED,
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &gmp_program_id,
    );

    let (mint_authority_pda, _) =
        Pubkey::find_program_address(&[MINT_AUTHORITY_SEED, mint.as_ref()], &ift_program_id);

    let sender_token_account = get_associated_token_address(&pending_transfer.sender, &mint);

    // Account order must match IFT's FinalizeTransfer struct
    let accounts = vec![
        AccountMeta::new_readonly(app_state_pda, false),
        AccountMeta::new(app_mint_state_pda, false),
        AccountMeta::new_readonly(ift_bridge_pda, false),
        AccountMeta::new(pending_transfer_pda, false),
        AccountMeta::new_readonly(gmp_result_pda, false),
        AccountMeta::new(mint, false),
        AccountMeta::new_readonly(mint_authority_pda, false),
        AccountMeta::new(sender_token_account, false),
        AccountMeta::new(fee_payer, true),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
        AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
    ];

    // Build instruction data: discriminator + client_id (string) + sequence (u64)
    let mut data = FINALIZE_TRANSFER_DISCRIMINATOR.to_vec();
    // Anchor serializes String as length-prefixed (u32 + bytes)
    let client_id_bytes = client_id.as_bytes();
    let client_id_len = u32::try_from(client_id_bytes.len()).expect("client_id length fits in u32");
    data.extend_from_slice(&client_id_len.to_le_bytes());
    data.extend_from_slice(client_id_bytes);
    data.extend_from_slice(&sequence.to_le_bytes());

    Instruction {
        program_id: ift_program_id,
        accounts,
        data,
    }
}
