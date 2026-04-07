//! GMP account extraction - delegates to shared `ibc_eureka_relayer_lib::utils::solana_gmp`.

pub use ibc_eureka_relayer_lib::utils::solana_gmp::{
    extract_gmp_accounts, extract_gmp_prefund_lamports, find_gmp_result_pda, GMP_PORT_ID,
    MAX_PREFUND_LAMPORTS, PROTOBUF_ENCODING,
};

use crate::constants::ABI_ENCODING;
use solana_ibc_proto::{GmpPacketData, Protobuf, RawGmpPacketData};

alloy::sol! {
    struct AbiGmpPacketData {
        string sender;
        string receiver;
        bytes salt;
        bytes payload;
        string memo;
    }
}

/// Decode a GMP packet from either protobuf or ABI encoding.
///
/// Returns `None` when decoding fails (treated as a non-GMP packet).
pub(crate) fn decode_gmp_packet(
    payload_value: &[u8],
    encoding: &str,
    _dest_port: &str,
) -> Option<GmpPacketData> {
    if encoding == ABI_ENCODING {
        use alloy::sol_types::SolValue;

        let abi = AbiGmpPacketData::abi_decode(payload_value).ok()?;
        let raw = RawGmpPacketData {
            sender: abi.sender,
            receiver: abi.receiver,
            salt: abi.salt.to_vec(),
            payload: abi.payload.to_vec(),
            memo: abi.memo,
        };
        GmpPacketData::try_from(raw).ok()
    } else {
        GmpPacketData::decode_vec(payload_value).ok()
    }
}
