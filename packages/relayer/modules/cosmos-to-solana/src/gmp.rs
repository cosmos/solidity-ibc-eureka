//! GMP (General Message Passing) account extraction utilities
//!
//! This module handles extraction of accounts from GMP payloads for Solana transaction building.
//! GMP enables cross-chain message execution by encoding Solana instructions in IBC packets.

use std::str::FromStr;

use anyhow::Result;
use prost::Message;
use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey};

use crate::constants::{GMP_PORT_ID, PROTOBUF_ENCODING};
use crate::proto::{GmpPacketData, SolanaInstruction};

/// Extract GMP accounts from packet payload
///
/// # Arguments
/// * `dest_port` - The destination port ID
/// * `encoding` - The payload encoding type
/// * `payload_value` - The raw payload data
/// * `source_client` - The source client ID
/// * `accounts` - Existing accounts list (used to extract IBC app program ID)
///
/// # Returns
/// Vector of GMP accounts
///
/// # Errors
/// Returns error if payload extraction fails
pub fn extract_gmp_accounts(
    dest_port: &str,
    encoding: &str,
    payload_value: &[u8],
    source_client: &str,
    accounts: &[AccountMeta],
) -> Result<Vec<AccountMeta>> {
    let (gmp_packet, receiver_pubkey) = match parse_gmp_packet(dest_port, encoding, payload_value) {
        Some(result) => result?,
        None => return Ok(Vec::new()),
    };

    // Derive account_state PDA
    let ibc_app_program_id = accounts
        .get(4)
        .map(|acc| acc.pubkey)
        .ok_or_else(|| anyhow::anyhow!("Missing ibc_app_program in existing accounts"))?;

    let (account_state_pda, _bump) = solana_ibc_types::GmpAccountState::pda(
        source_client,
        &gmp_packet.sender,
        &gmp_packet.salt,
        ibc_app_program_id,
    );

    let mut account_metas = vec![
        AccountMeta {
            pubkey: account_state_pda,
            is_signer: false,
            is_writable: true,
        },
        AccountMeta {
            pubkey: receiver_pubkey,
            is_signer: false,
            is_writable: true,
        },
    ];

    // Parse SolanaInstruction and extract additional accounts
    let solana_instruction = SolanaInstruction::decode(gmp_packet.payload.as_slice())
        .map_err(|e| anyhow::anyhow!("Failed to parse inner SolanaInstruction: {e}"))?;

    extract_instruction_accounts(&solana_instruction, &mut account_metas)?;

    tracing::info!(
        "Found {} additional accounts from GMP payload for port {}",
        account_metas.len(),
        dest_port
    );

    Ok(account_metas)
}

/// Parse and validate GMP packet from payload
///
/// Returns `None` if payload is not a GMP payload, `Some(Ok(...))` if valid, `Some(Err(...))` if invalid
fn parse_gmp_packet(
    port_id: &str,
    encoding: &str,
    payload_value: &[u8],
) -> Option<Result<(GmpPacketData, Pubkey)>> {
    // Only process GMP port payloads
    if port_id != GMP_PORT_ID || encoding != PROTOBUF_ENCODING {
        tracing::debug!(
            "No additional GMP accounts found in payload for port {} (non-GMP or parsing failed)",
            port_id
        );
        return None;
    }

    let Ok(gmp_packet) = GmpPacketData::decode(payload_value) else {
        tracing::debug!(
            "No additional GMP accounts found in payload for port {} (non-GMP or parsing failed)",
            port_id
        );
        return None;
    };

    // Parse receiver as Solana Pubkey (target program)
    let receiver_pubkey = match Pubkey::from_str(&gmp_packet.receiver) {
        Ok(pubkey) => pubkey,
        Err(e) => return Some(Err(anyhow::anyhow!("Invalid receiver pubkey: {e}"))),
    };

    tracing::info!(
        "GMP account extraction: receiver from packet = {} (target program)",
        receiver_pubkey
    );

    Some(Ok((gmp_packet, receiver_pubkey)))
}

/// Extract accounts from `SolanaInstruction` and add them to the account list
fn extract_instruction_accounts(
    instruction: &SolanaInstruction,
    account_metas: &mut Vec<AccountMeta>,
) -> Result<()> {
    for account_meta in &instruction.accounts {
        let pubkey = Pubkey::try_from(account_meta.pubkey.as_slice())
            .map_err(|e| anyhow::anyhow!("Invalid pubkey: {e}"))?;

        account_metas.push(AccountMeta {
            pubkey,
            is_signer: false,
            is_writable: account_meta.is_writable,
        });
    }
    Ok(())
}
