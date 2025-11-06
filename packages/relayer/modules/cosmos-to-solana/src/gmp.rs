//! GMP (General Message Passing) account extraction utilities
//!
//! This module handles extraction of accounts from GMP payloads for Solana transaction building.
//! GMP enables cross-chain message execution by encoding Solana instructions in IBC packets.

use std::str::FromStr;

use anyhow::Result;
use solana_ibc_types::ValidatedGmpPacketData;
use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey};

use crate::constants::{GMP_PORT_ID, PROTOBUF_ENCODING};
use crate::proto::{GmpPacketData, GmpSolanaPayload, ValidatedGMPSolanaPayload};

/// Extract GMP accounts from packet payload
///
/// # Arguments
/// * `dest_port` - The destination port ID
/// * `encoding` - The payload encoding type
/// * `payload_value` - The raw payload data
/// * `source_client` - The source client ID
/// * `ibc_app_program_id` - The IBC app program ID (GMP program)
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
    ibc_app_program_id: Pubkey,
) -> Result<Vec<AccountMeta>> {
    // Only process GMP port payloads
    if !is_gmp_payload(dest_port, encoding) {
        tracing::debug!(
            "No additional GMP accounts found in payload for port {} (non-GMP)",
            dest_port
        );
        return Ok(Vec::new());
    }

    // Decode and validate GMP packet
    let Some(validated_packet) = decode_gmp_packet(payload_value, dest_port) else {
        return Ok(Vec::new());
    };

    // Build account list from validated packet
    build_gmp_account_list(
        validated_packet,
        source_client,
        ibc_app_program_id,
        dest_port,
    )
}

/// Check if payload should be processed as GMP
fn is_gmp_payload(dest_port: &str, encoding: &str) -> bool {
    dest_port == GMP_PORT_ID && encoding == PROTOBUF_ENCODING
}

/// Decode and validate GMP packet, returning None on failure
fn decode_gmp_packet(payload_value: &[u8], dest_port: &str) -> Option<ValidatedGmpPacketData> {
    match GmpPacketData::decode_and_validate(payload_value) {
        Ok(packet) => Some(packet),
        Err(e) => {
            tracing::debug!(
                "Failed to decode/validate GMP packet for port {}: {e:?}",
                dest_port
            );
            None
        }
    }
}

/// Build the complete account list from validated GMP packet
fn build_gmp_account_list(
    validated_packet: ValidatedGmpPacketData,
    source_client: &str,
    ibc_app_program_id: Pubkey,
    dest_port: &str,
) -> Result<Vec<AccountMeta>> {
    // Parse receiver as Solana Pubkey (target program)
    let target_program = Pubkey::from_str(&validated_packet.receiver)
        .map_err(|e| anyhow::anyhow!("Invalid target program pubkey: {e}"))?;

    tracing::info!(
        "GMP account extraction: target program from packet = {}",
        target_program
    );

    // Construct typed ClientId
    let client_id = solana_ibc_types::ClientId::new(source_client)
        .map_err(|e| anyhow::anyhow!("Invalid client ID: {e:?}"))?;

    // Derive GMP account PDA with validated types
    let gmp_account = solana_ibc_types::GMPAccount::new(
        client_id,
        validated_packet.sender,
        validated_packet.salt,
        &ibc_app_program_id,
    );

    let (gmp_account_pda, _bump) = gmp_account.pda();

    // Include both gmp_account_pda and target_program in remaining accounts
    // The router passes these generically to GMP, which extracts target_program from [1]
    let mut account_metas = vec![
        AccountMeta {
            pubkey: gmp_account_pda,
            is_signer: false,
            is_writable: false, // readonly - stateless, no account creation
        },
        AccountMeta {
            pubkey: target_program,
            is_signer: false,
            is_writable: false, // target_program is readonly (it's a program ID)
        },
    ];

    // Parse and validate GMP Solana payload and extract additional accounts
    let gmp_solana_payload = GmpSolanaPayload::decode_and_validate(&validated_packet.payload)
        .map_err(|e| anyhow::anyhow!("Failed to validate GMP Solana payload: {e}"))?;

    add_instruction_accounts(&gmp_solana_payload, &mut account_metas);

    tracing::info!(
        "Found {} additional accounts from GMP payload for port {} (target program: {})",
        account_metas.len(),
        dest_port,
        target_program
    );

    Ok(account_metas)
}

/// Add accounts from `ValidatedGMPSolanaPayload` to the account list
fn add_instruction_accounts(
    instruction: &ValidatedGMPSolanaPayload,
    account_metas: &mut Vec<AccountMeta>,
) {
    for account_meta in &instruction.accounts {
        account_metas.push(AccountMeta {
            pubkey: account_meta.pubkey,
            is_signer: false,
            is_writable: account_meta.is_writable,
        });
    }
}
