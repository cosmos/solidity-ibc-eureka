//! Decoding `GmpPacketData` from ABI or protobuf encoding.

use anchor_lang::prelude::*;
use solana_ibc_proto::Protobuf;

use crate::errors::GMPError;

/// Decode `GmpPacketData` from either protobuf or ABI encoding based on the encoding string.
///
/// Used by `on_recv_packet`, `on_ack_packet` and `on_timeout_packet` to extract packet fields.
pub fn decode(value: &[u8], encoding: &str) -> Result<solana_ibc_proto::GmpPacketData> {
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

    #[test]
    fn test_decode_abi() {
        let sender = "cosmos1sender";
        let receiver = "11111111111111111111111111111111";
        let payload = vec![1, 2, 3, 4];

        let encoded = crate::abi::encode_abi_gmp_packet(sender, receiver, &[], &payload, "");
        let packet = decode(&encoded, crate::constants::ABI_ENCODING).unwrap();

        assert_eq!(&*packet.sender, sender);
        assert_eq!(&*packet.receiver, receiver);
        assert!(packet.salt.is_empty());
        assert_eq!(&*packet.payload, &payload);
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
        let encoded = crate::abi::encode_abi_gmp_packet("sender", "receiver", &[], &[], "");
        let result = decode(&encoded, crate::constants::ABI_ENCODING);
        assert!(result.is_err());
    }
}
