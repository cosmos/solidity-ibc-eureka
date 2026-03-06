//! GMP account extraction - delegates to shared `ibc_eureka_relayer_lib::utils::solana_gmp`.

pub use ibc_eureka_relayer_lib::utils::solana_gmp::{
    extract_gmp_accounts, find_gmp_result_pda, GMP_PORT_ID, PROTOBUF_ENCODING,
};

use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::proto::{GmpPacketData, GmpSolanaPayload, Protobuf};

/// Extract GMP PDA and `prefund_lamports` from packet payload.
///
/// Returns `None` when the payload is not a GMP packet.
/// The caller uses the returned `prefund_lamports` (capped) to build
/// a `system_program::transfer` instruction before `recv_packet`.
///
/// # Errors
///
/// Returns error if the GMP packet or inner Solana payload cannot be decoded.
pub fn extract_gmp_prefund_lamports(
    dest_port: &str,
    encoding: &str,
    payload_value: &[u8],
    dest_client: &str,
    ibc_app_program_id: Pubkey,
) -> Result<Option<(Pubkey, u64)>> {
    if !is_gmp_payload(dest_port, encoding) {
        return Ok(None);
    }

    let Some(packet) = decode_gmp_packet(payload_value, dest_port) else {
        return Ok(None);
    };

    let client_id = solana_ibc_types::ClientId::new(dest_client)
        .map_err(|e| anyhow::anyhow!("Invalid client ID: {e:?}"))?;

    let gmp_account = solana_ibc_types::GMPAccount::new(
        client_id,
        packet.sender,
        packet.salt,
        &ibc_app_program_id,
    );
    let (gmp_pda, _) = gmp_account.pda();

    let solana_payload = GmpSolanaPayload::decode_vec(&packet.payload)
        .map_err(|e| anyhow::anyhow!("Failed to decode GMP Solana payload: {e}"))?;

    Ok(Some((gmp_pda, solana_payload.prefund_lamports)))
}

fn is_gmp_payload(dest_port: &str, encoding: &str) -> bool {
    dest_port == GMP_PORT_ID && (encoding.is_empty() || encoding == PROTOBUF_ENCODING)
}

fn decode_gmp_packet(payload_value: &[u8], dest_port: &str) -> Option<GmpPacketData> {
    match GmpPacketData::decode_vec(payload_value) {
        Ok(packet) => Some(packet),
        Err(e) => {
            tracing::warn!("Failed to decode GMP packet for port {}: {e:?}", dest_port);
            None
        }
    }
}
