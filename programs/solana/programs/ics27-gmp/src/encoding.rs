use crate::errors::GMPError;
use alloy_sol_types::SolValue;
use anchor_lang::prelude::*;
use solana_ibc_proto::{
    GmpAcknowledgement, GmpPacketData, ProstMessage, Protobuf, RawGmpPacketData,
};

mod sol_types {
    alloy_sol_types::sol!("../../../../contracts/msgs/IICS27GMPMsgs.sol");
}

pub use sol_types::IICS27GMPMsgs::GMPAcknowledgement as GmpAcknowledgementAbi;
pub use sol_types::IICS27GMPMsgs::GMPPacketData as GmpPacketDataAbi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmpEncoding {
    Abi,
    Protobuf,
}

impl GmpEncoding {
    /// Wire-format string for the IBC payload encoding field.
    pub const ABI_STR: &str = "application/x-solidity-abi";
    pub const PROTOBUF_STR: &str = "application/x-protobuf";

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Abi => Self::ABI_STR,
            Self::Protobuf => Self::PROTOBUF_STR,
        }
    }

    pub fn encode_packet(self, data: GmpPacketData) -> Vec<u8> {
        match self {
            Self::Abi => GmpPacketDataAbi::from(data).abi_encode(),
            Self::Protobuf => data.encode_vec(),
        }
    }

    pub fn decode_packet(self, bytes: &[u8]) -> Result<RawGmpPacketData> {
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

    pub fn encode_ack(self, result: &[u8]) -> Vec<u8> {
        match self {
            Self::Abi => GmpAcknowledgementAbi::from(GmpAcknowledgement::success(result.to_vec()))
                .abi_encode(),
            Self::Protobuf => {
                // Proto3 omits empty bytes fields, so use the sentinel for empty results
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
            Self::ABI_STR => Ok(Self::Abi),
            Self::PROTOBUF_STR => Ok(Self::Protobuf),
            _ => Err(GMPError::InvalidEncoding.into()),
        }
    }
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
            GmpEncoding::try_from("application/x-solidity-abi").unwrap(),
            GmpEncoding::Abi
        );
        assert_eq!(
            GmpEncoding::try_from("application/x-protobuf").unwrap(),
            GmpEncoding::Protobuf
        );
        assert!(GmpEncoding::try_from("application/json").is_err());
    }

    #[test]
    fn encoding_round_trip_str() {
        assert_eq!(
            GmpEncoding::try_from(GmpEncoding::Abi.as_str()).unwrap(),
            GmpEncoding::Abi
        );
        assert_eq!(
            GmpEncoding::try_from(GmpEncoding::Protobuf.as_str()).unwrap(),
            GmpEncoding::Protobuf
        );
    }

    #[test]
    fn abi_round_trip() {
        let original = sample_packet_data();
        let encoded = GmpEncoding::Abi.encode_packet(original);
        let raw = GmpEncoding::Abi.decode_packet(&encoded).unwrap();
        assert_raw_matches_sample(&raw);

        let validated = GmpPacketData::try_from(raw).unwrap();
        assert_eq!(validated.sender.as_ref(), "solana_sender_pubkey");
    }

    #[test]
    fn protobuf_round_trip() {
        let original = sample_packet_data();
        let encoded = GmpEncoding::Protobuf.encode_packet(original);
        let raw = GmpEncoding::Protobuf.decode_packet(&encoded).unwrap();
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
        let encoded = GmpEncoding::Abi.encode_ack(&data);
        assert!(!encoded.is_empty());
        let decoded = GmpAcknowledgementAbi::abi_decode(&encoded).unwrap();
        assert_eq!(&decoded.result[..], &data);
    }

    #[test]
    fn ack_abi_empty_result() {
        // ABI always produces non-empty output, no sentinel needed
        let encoded = GmpEncoding::Abi.encode_ack(&[]);
        assert!(!encoded.is_empty());
        let decoded = GmpAcknowledgementAbi::abi_decode(&encoded).unwrap();
        assert!(decoded.result.is_empty());
    }

    #[test]
    fn ack_protobuf_round_trip() {
        let data = vec![1, 2, 3, 4];
        let encoded = GmpEncoding::Protobuf.encode_ack(&data);
        assert!(!encoded.is_empty());
        let decoded = GmpAcknowledgement::decode_vec(&encoded).unwrap();
        assert_eq!(decoded.result, data);
    }

    #[test]
    fn ack_protobuf_empty_result_uses_sentinel() {
        // Proto3 would produce zero bytes for empty result, sentinel keeps it non-empty
        let encoded = GmpEncoding::Protobuf.encode_ack(&[]);
        assert!(!encoded.is_empty());
        let decoded = GmpAcknowledgement::decode_vec(&encoded).unwrap();
        assert_eq!(decoded.result, vec![0]);
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
