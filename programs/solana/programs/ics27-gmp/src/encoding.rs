use crate::errors::GMPError;
use alloy_sol_types::SolValue;
use anchor_lang::prelude::*;
use solana_ibc_proto::{
    GmpAcknowledgement, GmpPacketData, ProstMessage, Protobuf, RawGmpPacketData,
};

mod sol_types {
    alloy_sol_types::sol!("../../../../contracts/msgs/IICS27GMPMsgs.sol");
}

pub(crate) use sol_types::IICS27GMPMsgs::GMPAcknowledgement as GmpAcknowledgementAbi;
pub use sol_types::IICS27GMPMsgs::GMPPacketData as GmpPacketDataAbi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GmpEncoding {
    Abi,
    Protobuf,
}

impl GmpEncoding {
    pub(crate) fn encode_packet(self, data: GmpPacketData) -> Vec<u8> {
        match self {
            Self::Abi => GmpPacketDataAbi::from(data).abi_encode(),
            Self::Protobuf => data.encode_vec(),
        }
    }

    pub(crate) fn decode_packet(self, bytes: &[u8]) -> Result<RawGmpPacketData> {
        match self {
            Self::Abi => {
                let abi =
                    GmpPacketDataAbi::abi_decode(bytes).map_err(|_| GMPError::InvalidPacketData)?;
                Ok(RawGmpPacketData::from(abi))
            }
            Self::Protobuf => {
                RawGmpPacketData::decode(bytes).map_err(|_| GMPError::InvalidPacketData.into())
            }
        }
    }

    pub(crate) fn encode_ack(self, result: &[u8]) -> Vec<u8> {
        match self {
            Self::Abi => GmpAcknowledgementAbi::from(GmpAcknowledgement::success(result.to_vec()))
                .abi_encode(),
            Self::Protobuf => {
                let ack = if result.is_empty() {
                    GmpAcknowledgement::empty_success()
                } else {
                    GmpAcknowledgement::success(result.to_vec())
                };
                ack.encode_to_vec()
            }
        }
    }
}

impl TryFrom<&str> for GmpEncoding {
    type Error = anchor_lang::error::Error;

    fn try_from(s: &str) -> Result<Self> {
        match s {
            crate::constants::ICS27_ENCODING_ABI => Ok(Self::Abi),
            crate::constants::ICS27_ENCODING_PROTOBUF => Ok(Self::Protobuf),
            _ => Err(GMPError::InvalidEncoding.into()),
        }
    }
}

/// Encode GMP packet data using the given encoding string.
pub fn encode_gmp_packet(data: GmpPacketData, encoding: &str) -> Result<Vec<u8>> {
    let enc = GmpEncoding::try_from(encoding)?;
    Ok(enc.encode_packet(data))
}

/// Decode GMP packet data from bytes using the given encoding string.
pub fn decode_gmp_packet(bytes: &[u8], encoding: &str) -> Result<RawGmpPacketData> {
    let enc = GmpEncoding::try_from(encoding)?;
    enc.decode_packet(bytes)
}

/// Encode a GMP acknowledgement from raw result bytes.
pub fn encode_gmp_ack(result: &[u8], encoding: &str) -> Result<Vec<u8>> {
    let enc = GmpEncoding::try_from(encoding)?;
    Ok(enc.encode_ack(result))
}

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

impl From<GmpAcknowledgement> for GmpAcknowledgementAbi {
    fn from(ack: GmpAcknowledgement) -> Self {
        Self {
            result: ack.result.into(),
        }
    }
}

impl From<GmpPacketDataAbi> for RawGmpPacketData {
    fn from(abi: GmpPacketDataAbi) -> Self {
        Self {
            sender: abi.sender,
            receiver: abi.receiver,
            salt: abi.salt.into(),
            payload: abi.payload.into(),
            memo: abi.memo,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{ICS27_ENCODING_ABI, ICS27_ENCODING_PROTOBUF};
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

    fn assert_raw_matches_sample(raw: &RawGmpPacketData) {
        assert_eq!(raw.sender, "solana_sender_pubkey");
        assert_eq!(raw.receiver, "0xabcdef1234567890abcdef1234567890abcdef12");
        assert_eq!(raw.salt, [1, 2, 3]);
        assert_eq!(raw.payload, [4, 5, 6, 7]);
        assert_eq!(raw.memo, "test memo");
    }

    #[test]
    fn encoding_from_str() {
        assert_eq!(
            GmpEncoding::try_from(ICS27_ENCODING_ABI).unwrap(),
            GmpEncoding::Abi
        );
        assert_eq!(
            GmpEncoding::try_from(ICS27_ENCODING_PROTOBUF).unwrap(),
            GmpEncoding::Protobuf
        );
        assert!(GmpEncoding::try_from("application/json").is_err());
    }

    #[test]
    fn abi_round_trip() {
        let original = sample_packet_data();
        let encoded = encode_gmp_packet(original, ICS27_ENCODING_ABI).unwrap();
        let raw = decode_gmp_packet(&encoded, ICS27_ENCODING_ABI).unwrap();
        assert_raw_matches_sample(&raw);

        let validated = GmpPacketData::try_from(raw).unwrap();
        assert_eq!(validated.sender.as_ref(), "solana_sender_pubkey");
    }

    #[test]
    fn protobuf_round_trip() {
        let original = sample_packet_data();
        let encoded = encode_gmp_packet(original, ICS27_ENCODING_PROTOBUF).unwrap();
        let raw = decode_gmp_packet(&encoded, ICS27_ENCODING_PROTOBUF).unwrap();
        assert_raw_matches_sample(&raw);

        let validated = GmpPacketData::try_from(raw).unwrap();
        assert_eq!(validated.sender.as_ref(), "solana_sender_pubkey");
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
    fn ack_abi_round_trip() {
        let data = vec![1, 2, 3, 4];
        let encoded = encode_gmp_ack(&data, ICS27_ENCODING_ABI).unwrap();
        assert!(!encoded.is_empty());
        let decoded = GmpAcknowledgementAbi::abi_decode(&encoded).unwrap();
        assert_eq!(&decoded.result[..], &data);
    }

    #[test]
    fn ack_abi_empty_result() {
        let encoded = encode_gmp_ack(&[], ICS27_ENCODING_ABI).unwrap();
        assert!(!encoded.is_empty());
        let decoded = GmpAcknowledgementAbi::abi_decode(&encoded).unwrap();
        assert!(decoded.result.is_empty());
    }

    #[test]
    fn ack_protobuf_round_trip() {
        let data = vec![1, 2, 3, 4];
        let encoded = encode_gmp_ack(&data, ICS27_ENCODING_PROTOBUF).unwrap();
        assert!(!encoded.is_empty());
        let decoded = GmpAcknowledgement::decode_vec(&encoded).unwrap();
        assert_eq!(decoded.result, data);
    }

    #[test]
    fn ack_protobuf_empty_result_uses_sentinel() {
        let encoded = encode_gmp_ack(&[], ICS27_ENCODING_PROTOBUF).unwrap();
        assert!(!encoded.is_empty());
        let decoded = GmpAcknowledgement::decode_vec(&encoded).unwrap();
        assert_eq!(decoded.result, vec![0]);
    }

    #[test]
    fn invalid_encoding_returns_error() {
        let original = sample_packet_data();
        assert!(encode_gmp_packet(original, "application/json").is_err());
        assert!(decode_gmp_packet(&[1, 2, 3], "application/json").is_err());
        assert!(encode_gmp_ack(&[1], "application/json").is_err());
    }

    #[test]
    fn abi_to_raw_then_validation_catches_invalid() {
        let abi = GmpPacketDataAbi {
            sender: String::new(), // Empty sender should fail validation
            receiver: "test".to_string(),
            salt: vec![].into(),
            payload: vec![1].into(),
            memo: String::new(),
        };
        let raw = RawGmpPacketData::from(abi);
        let result = GmpPacketData::try_from(raw);
        assert!(result.is_err());
    }
}
