//! Encoding and decoding `GmpPacketData` across ABI and protobuf formats.

use alloy_sol_types::SolValue;
use anchor_lang::prelude::*;
use solana_ibc_proto::{GmpPacketData, Protobuf};

use crate::errors::GMPError;
use crate::state::GmpEncoding;

/// Encode `GmpPacketData` into either ABI or protobuf format.
///
/// Returns the encoding string and the encoded bytes.
pub fn encode(packet_data: GmpPacketData, encoding: GmpEncoding) -> (&'static str, Vec<u8>) {
    match encoding {
        GmpEncoding::Abi => (
            crate::constants::ABI_ENCODING,
            crate::abi::AbiGmpPacketData::from(packet_data).abi_encode(),
        ),
        GmpEncoding::Protobuf => (crate::constants::ICS27_ENCODING, packet_data.encode_vec()),
    }
}

/// Decode `GmpPacketData` from either protobuf or ABI encoding based on the encoding string.
///
/// Used by `on_recv_packet`, `on_ack_packet` and `on_timeout_packet` to extract packet fields.
pub fn decode(value: &[u8], encoding: &str) -> Result<GmpPacketData> {
    match encoding {
        crate::constants::ABI_ENCODING => {
            let raw = crate::abi::abi_decode_gmp_packet_data(value)?;
            raw.try_into()
                .map_err(|_| error!(GMPError::InvalidPacketData))
        }
        crate::constants::ICS27_ENCODING => {
            solana_ibc_proto::GmpPacketData::decode(value).map_err(|e| {
                msg!("GMP protobuf decode failed: {}", e);
                error!(GMPError::InvalidPacketData)
            })
        }
        _ => Err(error!(GMPError::InvalidEncoding)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::AbiGmpPacketData;
    use alloy_sol_types::SolValue;

    fn create_test_packet_data() -> GmpPacketData {
        let raw = solana_ibc_proto::RawGmpPacketData {
            sender: "cosmos1sender".to_string(),
            receiver: "11111111111111111111111111111111".to_string(),
            salt: vec![1, 2, 3],
            payload: vec![4, 5, 6],
            memo: "hello".to_string(),
        };
        raw.try_into().unwrap()
    }

    #[test]
    fn test_encode_decode_abi_roundtrip() {
        let packet = create_test_packet_data();
        let (encoding, bytes) = encode(packet, GmpEncoding::Abi);

        assert_eq!(encoding, crate::constants::ABI_ENCODING);

        let decoded = decode(&bytes, encoding).unwrap();
        assert_eq!(&*decoded.sender, "cosmos1sender");
        assert_eq!(&*decoded.receiver, "11111111111111111111111111111111");
        assert_eq!(&*decoded.salt, &[1, 2, 3]);
        assert_eq!(&*decoded.payload, &[4, 5, 6]);
        assert_eq!(&*decoded.memo, "hello");
    }

    #[test]
    fn test_encode_decode_protobuf_roundtrip() {
        let packet = create_test_packet_data();
        let (encoding, bytes) = encode(packet, GmpEncoding::Protobuf);

        assert_eq!(encoding, crate::constants::ICS27_ENCODING);

        let decoded = decode(&bytes, encoding).unwrap();
        assert_eq!(&*decoded.sender, "cosmos1sender");
        assert_eq!(&*decoded.receiver, "11111111111111111111111111111111");
        assert_eq!(&*decoded.salt, &[1, 2, 3]);
        assert_eq!(&*decoded.payload, &[4, 5, 6]);
        assert_eq!(&*decoded.memo, "hello");
    }

    #[test]
    fn test_decode_abi() {
        let packet = create_test_packet_data();
        let (_, encoded) = encode(packet, GmpEncoding::Abi);
        let decoded = decode(&encoded, crate::constants::ABI_ENCODING).unwrap();

        assert_eq!(&*decoded.sender, "cosmos1sender");
        assert_eq!(&*decoded.payload, &[4, 5, 6]);
    }

    #[test]
    fn test_decode_protobuf() {
        use solana_ibc_proto::ProstMessage;

        let raw = solana_ibc_proto::RawGmpPacketData {
            sender: "cosmos1sender".to_string(),
            receiver: "11111111111111111111111111111111".to_string(),
            salt: vec![1, 2, 3],
            payload: vec![4, 5, 6],
            memo: "hello".to_string(),
        };
        let encoded = raw.encode_to_vec();

        let packet = decode(&encoded, crate::constants::ICS27_ENCODING).unwrap();

        assert_eq!(&*packet.sender, "cosmos1sender");
        assert_eq!(&*packet.receiver, "11111111111111111111111111111111");
        assert_eq!(&*packet.salt, &[1, 2, 3]);
        assert_eq!(&*packet.payload, &[4, 5, 6]);
        assert_eq!(&*packet.memo, "hello");
    }

    #[test]
    fn test_decode_invalid_encoding() {
        let result = decode(&[1, 2, 3], "application/json");
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_abi_invalid_bytes() {
        let result = decode(&[0xFF; 10], crate::constants::ABI_ENCODING);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_protobuf_invalid_bytes() {
        let result = decode(&[0xFF; 10], crate::constants::ICS27_ENCODING);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_abi_empty_payload_rejected() {
        let abi_data = AbiGmpPacketData {
            sender: "sender".into(),
            receiver: "receiver".into(),
            salt: vec![].into(),
            payload: vec![].into(),
            memo: String::new(),
        };
        let encoded = abi_data.abi_encode();
        let result = decode(&encoded, crate::constants::ABI_ENCODING);
        assert!(result.is_err());
    }
}
