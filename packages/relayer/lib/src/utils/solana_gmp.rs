//! GMP (General Message Passing) account extraction utilities.
//!
//! Handles both protobuf-encoded (Cosmos-originated) and ABI-encoded
//! (Ethereum-originated) GMP payloads. Decodes packet data and builds
//! the remaining accounts needed for Solana `recv_packet` transactions.

use std::str::FromStr;

use alloy::sol_types::SolValue;
use anyhow::Result;
use solana_sdk::{instruction::AccountMeta, pubkey::Pubkey};

use solana_ibc_proto::{GmpPacketData, GmpSolanaPayload, Protobuf};

/// GMP (General Message Passing) port identifier.
pub const GMP_PORT_ID: &str = "gmpport";

/// Protobuf encoding type for GMP packets.
pub const PROTOBUF_ENCODING: &str = "application/x-protobuf";

/// ABI encoding identifier used by Ethereum ICS27 GMP.
pub const ABI_ENCODING: &str = "application/x-solidity-abi";

/// Packed account entry size: pubkey(32) + `is_signer`(1) + `is_writable`(1)
const PACKED_ACCOUNT_SIZE: usize = 34;

// ABI type matching Solidity's `IICS27GMPMsgs.GMPPacketData`.
alloy::sol! {
    struct AbiGmpPacketData {
        string sender;
        string receiver;
        bytes salt;
        bytes payload;
        string memo;
    }
}

// ABI type matching the inner GmpSolanaPayload encoding:
// `abi.encode(bytes packedAccounts, bytes instructionData, uint32 payerPosition)`
alloy::sol! {
    struct AbiGmpSolanaPayload {
        bytes packedAccounts;
        bytes instructionData;
        uint32 payerPosition;
    }
}

/// Extract GMP remaining accounts from a packet payload.
///
/// Dispatches to protobuf or ABI decoding based on `encoding`.
/// Returns an empty vec for non-GMP payloads.
pub fn extract_gmp_accounts(
    dest_port: &str,
    encoding: &str,
    payload_value: &[u8],
    dest_client: &str,
    gmp_program_id: Pubkey,
) -> Result<Vec<AccountMeta>> {
    if dest_port != GMP_PORT_ID {
        return Ok(Vec::new());
    }

    let is_protobuf = encoding.is_empty() || encoding == PROTOBUF_ENCODING;
    let is_abi = encoding == ABI_ENCODING;

    if is_protobuf {
        decode_protobuf_gmp_packet(payload_value, dest_client, gmp_program_id)
    } else if is_abi {
        decode_abi_gmp_packet(payload_value, dest_client, gmp_program_id)
    } else {
        tracing::debug!("Skipping GMP payload with unknown encoding: '{encoding}'");
        Ok(Vec::new())
    }
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

// ---------------------------------------------------------------------------
// Shared
// ---------------------------------------------------------------------------

/// Derive the GMP PDA and build the base account list
/// `[gmp_pda, target_program, ...execution_accounts]`.
fn build_gmp_account_list(
    sender: solana_ibc_proto::Sender,
    receiver: &str,
    salt: solana_ibc_proto::Salt,
    dest_client: &str,
    gmp_program_id: Pubkey,
    execution_accounts: impl Iterator<Item = AccountMeta>,
) -> Result<Vec<AccountMeta>> {
    let target_program = Pubkey::from_str(receiver)
        .map_err(|e| anyhow::anyhow!("Invalid target program pubkey: {e}"))?;

    let client_id = solana_ibc_types::ClientId::new(dest_client)
        .map_err(|e| anyhow::anyhow!("Invalid client ID: {e:?}"))?;

    let gmp_account =
        solana_ibc_types::GMPAccount::new(client_id, sender.clone(), salt, &gmp_program_id);
    let (gmp_pda, _) = gmp_account.pda();

    tracing::info!(
        gmp_pda = %gmp_pda,
        %sender,
        dest_client,
        "GMP PDA derived"
    );

    let mut accounts = vec![
        AccountMeta {
            pubkey: gmp_pda,
            is_signer: false,
            is_writable: false,
        },
        AccountMeta {
            pubkey: target_program,
            is_signer: false,
            is_writable: false,
        },
    ];

    accounts.extend(execution_accounts);

    tracing::debug!(
        num_accounts = accounts.len(),
        gmp_pda = %gmp_pda,
        target = %target_program,
        "GMP account list built"
    );

    Ok(accounts)
}

// ---------------------------------------------------------------------------
// Protobuf path
// ---------------------------------------------------------------------------

fn decode_protobuf_gmp_packet(
    payload_value: &[u8],
    dest_client: &str,
    gmp_program_id: Pubkey,
) -> Result<Vec<AccountMeta>> {
    let packet = match GmpPacketData::decode_vec(payload_value) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Failed to decode protobuf GMP packet: {e:?}");
            return Ok(Vec::new());
        }
    };

    let gmp_solana_payload = GmpSolanaPayload::decode_vec(&packet.payload)
        .map_err(|e| anyhow::anyhow!("Failed to decode GMP Solana payload: {e}"))?;

    // is_signer is always false at the transaction level.
    // PDAs sign via invoke_signed during CPI.
    let execution_accounts = gmp_solana_payload.accounts.iter().map(|a| AccountMeta {
        pubkey: a.pubkey,
        is_signer: false,
        is_writable: a.is_writable,
    });

    build_gmp_account_list(
        packet.sender,
        &packet.receiver,
        packet.salt,
        dest_client,
        gmp_program_id,
        execution_accounts,
    )
}

// ---------------------------------------------------------------------------
// ABI path
// ---------------------------------------------------------------------------

fn decode_abi_gmp_packet(
    payload_value: &[u8],
    dest_client: &str,
    gmp_program_id: Pubkey,
) -> Result<Vec<AccountMeta>> {
    let abi_gmp: AbiGmpPacketData = SolValue::abi_decode(payload_value)
        .map_err(|e| anyhow::anyhow!("Failed to ABI decode GMPPacketData: {e}"))?;

    // Use abi_decode_params (not abi_decode) because constructMintCall returns
    // abi.encode(bytes, bytes, uint32) — three separate params without an outer
    // tuple offset.
    let abi_solana: AbiGmpSolanaPayload = SolValue::abi_decode_params(&abi_gmp.payload)
        .map_err(|e| anyhow::anyhow!("Failed to ABI decode GmpSolanaPayload: {e}"))?;

    let packed = &abi_solana.packedAccounts;
    if packed.len() % PACKED_ACCOUNT_SIZE != 0 {
        anyhow::bail!(
            "Packed accounts length {} is not a multiple of {}",
            packed.len(),
            PACKED_ACCOUNT_SIZE
        );
    }

    let sender: solana_ibc_proto::Sender = abi_gmp
        .sender
        .clone()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid sender"))?;
    let salt: solana_ibc_proto::Salt = abi_gmp
        .salt
        .to_vec()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid salt"))?;

    let execution_accounts = packed
        .chunks_exact(PACKED_ACCOUNT_SIZE)
        .map(|chunk| {
            let pubkey_bytes: [u8; 32] = chunk[..32]
                .try_into()
                .expect("chunk is exactly PACKED_ACCOUNT_SIZE");
            AccountMeta {
                pubkey: Pubkey::from(pubkey_bytes),
                is_signer: false,
                is_writable: chunk[33] != 0,
            }
        });

    build_gmp_account_list(
        sender,
        &abi_gmp.receiver,
        salt,
        dest_client,
        gmp_program_id,
        execution_accounts,
    )
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

        let result2 = find_gmp_result_pda(GMP_PORT_ID, "client-0", 1, program_id);
        assert_eq!(result, result2);
    }
}
