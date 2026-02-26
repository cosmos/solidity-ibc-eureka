//! GMP (General Message Passing) account extraction utilities.
//!
//! This module handles extraction of accounts from GMP payloads for Solana transaction building.
//! GMP enables cross-chain message execution by encoding Solana instructions in IBC packets.

use std::str::FromStr;

use anyhow::Result;
use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey};

use solana_ibc_proto::{GmpPacketData, GmpSolanaPayload, Protobuf};

/// GMP (General Message Passing) port identifier.
pub const GMP_PORT_ID: &str = "gmpport";

/// Protobuf encoding type for GMP packets.
pub const PROTOBUF_ENCODING: &str = "application/x-protobuf";

/// Extract GMP accounts from packet payload.
///
/// # Arguments
/// * `dest_port` - The destination port ID
/// * `encoding` - The payload encoding type
/// * `payload_value` - The raw payload data
/// * `dest_client` - The destination client ID (local client on Solana)
/// * `ibc_app_program_id` - The IBC app program ID (GMP program)
///
/// # Returns
/// Vector of GMP accounts needed for the transaction.
///
/// # Errors
/// Returns error if payload extraction fails.
pub fn extract_gmp_accounts(
    dest_port: &str,
    encoding: &str,
    payload_value: &[u8],
    dest_client: &str,
    ibc_app_program_id: Pubkey,
) -> Result<Vec<AccountMeta>> {
    if !is_gmp_payload(dest_port, encoding) {
        tracing::debug!(
            "Skipping non-GMP payload: dest_port='{}', encoding='{}'",
            dest_port,
            encoding
        );
        return Ok(Vec::new());
    }

    let Some(validated_packet) = decode_gmp_packet(payload_value, dest_port) else {
        return Ok(Vec::new());
    };

    build_gmp_account_list(validated_packet, dest_client, ibc_app_program_id)
}

/// Check if payload should be processed as GMP.
fn is_gmp_payload(dest_port: &str, encoding: &str) -> bool {
    // Accept empty encoding for Cosmos compatibility
    dest_port == GMP_PORT_ID && (encoding.is_empty() || encoding == PROTOBUF_ENCODING)
}

/// Decode and validate GMP packet, returning None on failure.
fn decode_gmp_packet(payload_value: &[u8], dest_port: &str) -> Option<GmpPacketData> {
    match GmpPacketData::decode_vec(payload_value) {
        Ok(packet) => Some(packet),
        Err(e) => {
            tracing::warn!("Failed to decode GMP packet for port {}: {e:?}", dest_port);
            None
        }
    }
}

/// Build the complete account list from GMP packet.
fn build_gmp_account_list(
    packet: GmpPacketData,
    dest_client: &str,
    ibc_app_program_id: Pubkey,
) -> Result<Vec<AccountMeta>> {
    let target_program = Pubkey::from_str(&packet.receiver)
        .map_err(|e| anyhow::anyhow!("Invalid target program pubkey: {e}"))?;

    let client_id = solana_ibc_types::ClientId::new(dest_client)
        .map_err(|e| anyhow::anyhow!("Invalid client ID: {e:?}"))?;

    let salt_clone = packet.salt.clone();
    let gmp_account = solana_ibc_types::GMPAccount::new(
        client_id,
        packet.sender.clone(),
        packet.salt,
        &ibc_app_program_id,
    );

    let (gmp_account_pda, _bump) = gmp_account.pda();

    // Critical for debugging PrivilegeEscalation - keep at INFO
    let salt_bytes: &[u8] = &salt_clone;
    tracing::info!(
        "GMP: client='{}', sender='{}', salt={:?} ({} bytes) → pda={}",
        dest_client,
        packet.sender,
        salt_bytes,
        salt_bytes.len(),
        gmp_account_pda
    );

    // Include both gmp_account_pda and target_program in remaining accounts.
    // The router passes these generically to GMP, which extracts target_program from [1].
    let mut account_metas = vec![
        AccountMeta {
            pubkey: gmp_account_pda,
            is_signer: false,
            is_writable: false,
        },
        AccountMeta {
            pubkey: target_program,
            is_signer: false,
            is_writable: false,
        },
    ];

    let gmp_solana_payload = GmpSolanaPayload::decode_vec(&packet.payload)
        .map_err(|e| anyhow::anyhow!("Failed to decode/validate GMP Solana payload: {e}"))?;

    // Note: is_signer is always false for transaction-level accounts.
    // Accounts that need to sign during CPI (e.g., PDAs) will have their
    // signing authority established via `invoke_signed` by the GMP program.
    for account_meta in &gmp_solana_payload.accounts {
        account_metas.push(AccountMeta {
            pubkey: account_meta.pubkey,
            is_signer: false,
            is_writable: account_meta.is_writable,
        });
    }

    tracing::debug!(
        "GMP extraction: {} accounts, gmp_pda={}, target={}",
        account_metas.len(),
        gmp_account_pda,
        target_program
    );

    Ok(account_metas)
}

/// Find the GMP call result PDA for a given packet.
///
/// Returns `Some(pda)` if the packet is from the GMP port, where the PDA
/// stores the acknowledgement or timeout result of a GMP call.
///
/// Returns `None` for non-GMP ports or invalid sequence (0).
#[must_use]
pub fn find_gmp_result_pda(
    source_port: &str,
    source_client: &str,
    sequence: u64,
    gmp_program_id: Pubkey,
) -> Option<Pubkey> {
    if source_port != GMP_PORT_ID || sequence == 0 {
        return None;
    }

    let (pda, _bump) =
        solana_ibc_types::GMPCallResult::pda(source_client, sequence, &gmp_program_id);
    Some(pda)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_gmp_result_pda_returns_none_for_non_gmp_port() {
        let program_id = Pubkey::new_unique();
        assert!(find_gmp_result_pda("transfer", "client-0", 1, program_id).is_none());
    }

    #[test]
    fn find_gmp_result_pda_returns_none_for_zero_sequence() {
        let program_id = Pubkey::new_unique();
        assert!(find_gmp_result_pda(GMP_PORT_ID, "client-0", 0, program_id).is_none());
    }

    #[test]
    fn find_gmp_result_pda_returns_pda_for_valid_gmp_packet() {
        let program_id = Pubkey::new_unique();
        let result = find_gmp_result_pda(GMP_PORT_ID, "client-0", 1, program_id);
        assert!(result.is_some());

        // Verify deterministic derivation
        let result2 = find_gmp_result_pda(GMP_PORT_ID, "client-0", 1, program_id);
        assert_eq!(result, result2);
    }
}
