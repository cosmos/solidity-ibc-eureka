use crate::constants::{ICS27_ENCODING_ABI, ICS27_ENCODING_PROTOBUF};
use crate::errors::GMPError;
use anchor_lang::prelude::*;
use solana_ibc_proto::{GmpPacketData, ProstMessage, Protobuf, RawGmpPacketData};

mod sol_types {
    alloy_sol_types::sol!("../../../../contracts/msgs/IICS27GMPMsgs.sol");
}

pub use sol_types::IICS27GMPMsgs::GMPPacketData as GmpPacketDataAbi;

impl From<GmpPacketData> for GmpPacketDataAbi {
    fn from(data: GmpPacketData) -> Self {
        Self {
            sender: data.sender.into_string(),
            receiver: data.receiver.into_string(),
            salt: data.salt.into_vec().into(),
            payload: data.payload.into_inner().into(),
            memo: data.memo.into_string(),
        }
    }
}

impl TryFrom<GmpPacketDataAbi> for GmpPacketData {
    type Error = anchor_lang::error::Error;

    fn try_from(abi: GmpPacketDataAbi) -> Result<Self> {
        let raw = RawGmpPacketData {
            sender: abi.sender,
            receiver: abi.receiver,
            salt: abi.salt.into(),
            payload: abi.payload.into(),
            memo: abi.memo,
        };
        Self::try_from(raw).map_err(|e| {
            msg!("GMP packet validation failed: {}", e);
            GMPError::InvalidPacketData.into()
        })
    }
}

pub fn encode_gmp_packet(data: GmpPacketData, encoding: &str) -> Result<Vec<u8>> {
    match encoding {
        ICS27_ENCODING_ABI => {
            use alloy_sol_types::SolValue;
            Ok(GmpPacketDataAbi::from(data).abi_encode())
        }
        ICS27_ENCODING_PROTOBUF => Ok(data.encode_vec()),
        _ => Err(GMPError::InvalidEncoding.into()),
    }
}

pub fn decode_gmp_packet(bytes: &[u8], encoding: &str) -> Result<GmpPacketData> {
    match encoding {
        ICS27_ENCODING_ABI => {
            use alloy_sol_types::SolValue;
            let abi =
                GmpPacketDataAbi::abi_decode(bytes).map_err(|_| GMPError::InvalidPacketData)?;
            GmpPacketData::try_from(abi)
        }
        ICS27_ENCODING_PROTOBUF => {
            let raw = RawGmpPacketData::decode(bytes).map_err(|_| GMPError::InvalidPacketData)?;
            GmpPacketData::try_from(raw).map_err(|e| {
                msg!("GMP packet validation failed: {}", e);
                GMPError::InvalidPacketData.into()
            })
        }
        _ => Err(GMPError::InvalidEncoding.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_sol_types::SolValue;

    fn sample_packet_data() -> GmpPacketData {
        let raw = RawGmpPacketData {
            sender: "solana_sender_pubkey".to_string(),
            receiver: "0xabcdef1234567890abcdef1234567890abcdef12".to_string(),
            salt: vec![1, 2, 3],
            payload: vec![4, 5, 6, 7],
            memo: "test memo".to_string(),
        };
        GmpPacketData::try_from(raw).unwrap()
    }

    #[test]
    fn abi_round_trip() {
        let original = sample_packet_data();
        let encoded = encode_gmp_packet(original, ICS27_ENCODING_ABI).unwrap();
        let decoded = decode_gmp_packet(&encoded, ICS27_ENCODING_ABI).unwrap();

        assert_eq!(decoded.sender.as_ref(), "solana_sender_pubkey");
        assert_eq!(
            decoded.receiver.as_ref(),
            "0xabcdef1234567890abcdef1234567890abcdef12"
        );
        assert_eq!(&decoded.salt[..], &[1, 2, 3]);
        assert_eq!(&decoded.payload[..], &[4, 5, 6, 7]);
        assert_eq!(decoded.memo.as_ref(), "test memo");
    }

    #[test]
    fn protobuf_round_trip() {
        let original = sample_packet_data();
        let encoded = encode_gmp_packet(original, ICS27_ENCODING_PROTOBUF).unwrap();
        let decoded = decode_gmp_packet(&encoded, ICS27_ENCODING_PROTOBUF).unwrap();

        assert_eq!(decoded.sender.as_ref(), "solana_sender_pubkey");
        assert_eq!(
            decoded.receiver.as_ref(),
            "0xabcdef1234567890abcdef1234567890abcdef12"
        );
        assert_eq!(&decoded.salt[..], &[1, 2, 3]);
        assert_eq!(&decoded.payload[..], &[4, 5, 6, 7]);
        assert_eq!(decoded.memo.as_ref(), "test memo");
    }

    #[test]
    fn invalid_encoding_rejected() {
        let data = sample_packet_data();
        let result = encode_gmp_packet(data, "application/json");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_encoding_decode_rejected() {
        let result = decode_gmp_packet(&[0, 1, 2], "application/json");
        assert!(result.is_err());
    }

    #[test]
    fn abi_encode_matches_solidity_layout() {
        let abi = GmpPacketDataAbi {
            sender: "test".to_string(),
            receiver: "test".to_string(),
            salt: vec![].into(),
            payload: vec![1].into(),
            memo: String::new(),
        };
        let encoded = abi.abi_encode();
        // First 32 bytes should be offset pointer 0x20 for the dynamic struct
        assert_eq!(encoded[31], 0x20);
    }

    #[test]
    fn from_gmp_packet_data_preserves_fields() {
        let original = sample_packet_data();
        let abi: GmpPacketDataAbi = original.into();

        assert_eq!(abi.sender, "solana_sender_pubkey");
        assert_eq!(abi.receiver, "0xabcdef1234567890abcdef1234567890abcdef12");
        assert_eq!(&abi.salt[..], &[1, 2, 3]);
        assert_eq!(&abi.payload[..], &[4, 5, 6, 7]);
        assert_eq!(abi.memo, "test memo");
    }

    #[test]
    fn try_from_abi_validates_constraints() {
        let abi = GmpPacketDataAbi {
            sender: String::new(), // Empty sender should fail validation
            receiver: "test".to_string(),
            salt: vec![].into(),
            payload: vec![1].into(),
            memo: String::new(),
        };
        let result = GmpPacketData::try_from(abi);
        assert!(result.is_err());
    }
}
