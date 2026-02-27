//! ABI encoding and decoding for GMP types using `alloy-sol-types`.

use alloy_sol_types::SolValue;
use anchor_lang::prelude::*;

use crate::errors::GMPError;
use crate::proto::GmpSolanaPayload;

alloy_sol_types::sol! {
    struct AbiGmpPacketData {
        string sender;
        string receiver;
        bytes salt;
        bytes payload;
        string memo;
    }

    /// Encoded via `abi.encode(bytes, bytes, uint32)` (three separate params, not a struct).
    struct AbiGmpSolanaPayload {
        bytes packedAccounts;
        bytes instructionData;
        uint32 payerPosition;
    }
}

/// Size of a packed account entry: pubkey(32) + `is_signer`(1) + `is_writable`(1)
const PACKED_ACCOUNT_SIZE: usize = 34;

impl TryFrom<AbiGmpSolanaPayload> for solana_ibc_proto::RawGmpSolanaPayload {
    type Error = GMPError;

    fn try_from(abi: AbiGmpSolanaPayload) -> std::result::Result<Self, Self::Error> {
        let chunks = abi.packedAccounts.chunks_exact(PACKED_ACCOUNT_SIZE);
        if !chunks.remainder().is_empty() {
            return Err(GMPError::InvalidAbiEncoding);
        }
        let accounts = chunks
            .map(|chunk| solana_ibc_proto::RawSolanaAccountMeta {
                pubkey: chunk[..32].to_vec(),
                is_signer: chunk[32] != 0,
                is_writable: chunk[33] != 0,
            })
            .collect();
        Ok(Self {
            accounts,
            data: abi.instructionData.into(),
            payer_position: Some(abi.payerPosition),
        })
    }
}

/// Decode ABI-encoded `GmpSolanaPayload(bytes, bytes, uint32)`.
///
/// Uses `abi_decode_params` because Solidity encodes this with
/// `abi.encode(bytes, bytes, uint32)` (three separate params without an outer tuple offset).
pub fn decode_abi_gmp_solana_payload(data: &[u8]) -> Result<GmpSolanaPayload> {
    let decoded = AbiGmpSolanaPayload::abi_decode_params(data)
        .map_err(|_| error!(GMPError::InvalidAbiEncoding))?;
    let raw: solana_ibc_proto::RawGmpSolanaPayload =
        decoded.try_into().map_err(|e: GMPError| error!(e))?;
    raw.try_into()
        .map_err(|_| error!(GMPError::InvalidAbiEncoding))
}

impl From<solana_ibc_proto::GmpPacketData> for AbiGmpPacketData {
    fn from(data: solana_ibc_proto::GmpPacketData) -> Self {
        Self {
            sender: data.sender.to_string(),
            receiver: data.receiver.to_string(),
            salt: data.salt.to_vec().into(),
            payload: data.payload.to_vec().into(),
            memo: data.memo.to_string(),
        }
    }
}

impl From<AbiGmpPacketData> for solana_ibc_proto::RawGmpPacketData {
    fn from(abi: AbiGmpPacketData) -> Self {
        Self {
            sender: abi.sender,
            receiver: abi.receiver,
            salt: abi.salt.into(),
            payload: abi.payload.into(),
            memo: abi.memo,
        }
    }
}

/// ABI-decode raw bytes into `RawGmpPacketData`.
pub fn abi_decode_gmp_packet_data(value: &[u8]) -> Result<solana_ibc_proto::RawGmpPacketData> {
    let decoded =
        AbiGmpPacketData::abi_decode(value).map_err(|_| error!(GMPError::InvalidAbiEncoding))?;
    Ok(decoded.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_abi_gmp_solana_payload_roundtrip() {
        let pubkey1 = Pubkey::new_unique();
        let pubkey2 = Pubkey::new_unique();

        let mut packed = Vec::new();
        packed.extend_from_slice(&pubkey1.to_bytes());
        packed.push(1); // is_signer
        packed.push(0); // is_writable
        packed.extend_from_slice(&pubkey2.to_bytes());
        packed.push(0); // is_signer
        packed.push(1); // is_writable

        let instr_data = vec![0xAA, 0xBB, 0xCC, 0xDD];

        let encoded = AbiGmpSolanaPayload {
            packedAccounts: packed.into(),
            instructionData: instr_data.clone().into(),
            payerPosition: 8,
        }
        .abi_encode_params();

        let decoded = decode_abi_gmp_solana_payload(&encoded).unwrap();

        assert_eq!(decoded.accounts.len(), 2);
        assert_eq!(decoded.accounts[0].pubkey, pubkey1);
        assert!(decoded.accounts[0].is_signer);
        assert!(!decoded.accounts[0].is_writable);
        assert_eq!(decoded.accounts[1].pubkey, pubkey2);
        assert!(!decoded.accounts[1].is_signer);
        assert!(decoded.accounts[1].is_writable);
        assert_eq!(decoded.data, instr_data);
        assert_eq!(decoded.payer_position, Some(8));
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_empty_accounts() {
        let instr_data = vec![1, 2, 3];
        let encoded = AbiGmpSolanaPayload {
            packedAccounts: Vec::new().into(),
            instructionData: instr_data.clone().into(),
            payerPosition: 0,
        }
        .abi_encode_params();

        let decoded = decode_abi_gmp_solana_payload(&encoded).unwrap();

        assert!(decoded.accounts.is_empty());
        assert_eq!(decoded.data, instr_data);
        assert_eq!(decoded.payer_position, Some(0));
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_too_short() {
        let data = vec![0u8; 95]; // less than 3 words
        assert!(decode_abi_gmp_solana_payload(&data).is_err());
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_misaligned_accounts() {
        let bad_packed = vec![0u8; 35]; // not a multiple of 34
        let encoded = AbiGmpSolanaPayload {
            packedAccounts: bad_packed.into(),
            instructionData: vec![1].into(),
            payerPosition: 0,
        }
        .abi_encode_params();

        assert!(decode_abi_gmp_solana_payload(&encoded).is_err());
    }

    fn build_packet_data(
        sender: &str,
        receiver: &str,
        salt: &[u8],
        payload: &[u8],
        memo: &str,
    ) -> solana_ibc_proto::GmpPacketData {
        solana_ibc_proto::RawGmpPacketData {
            sender: sender.to_string(),
            receiver: receiver.to_string(),
            salt: salt.to_vec(),
            payload: payload.to_vec(),
            memo: memo.to_string(),
        }
        .try_into()
        .unwrap()
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let sender = "0x1234567890abcdef";
        let receiver = "So1ana1111111111111111111111111111111111111";
        let salt = b"test-salt";
        let payload = b"some payload data";
        let memo = "hello memo";

        let packet = build_packet_data(sender, receiver, salt, payload, memo);
        let encoded = AbiGmpPacketData::from(packet).abi_encode();
        let decoded = AbiGmpPacketData::abi_decode(&encoded).expect("failed to decode ABI packet");

        assert_eq!(decoded.sender, sender);
        assert_eq!(decoded.receiver, receiver);
        assert_eq!(decoded.salt.as_ref(), salt);
        assert_eq!(decoded.payload.as_ref(), payload);
        assert_eq!(decoded.memo, memo);
    }

    #[test]
    fn test_decode_too_short() {
        assert!(AbiGmpPacketData::abi_decode(&[0u8; 31]).is_err());
    }

    #[test]
    fn test_abi_decode_gmp_packet_data_roundtrip() {
        let sender = "cosmos1sender";
        let receiver = "11111111111111111111111111111111";
        let payload = vec![1, 2, 3, 4];

        let packet = build_packet_data(sender, receiver, &[], &payload, "");
        let encoded = AbiGmpPacketData::from(packet).abi_encode();
        let raw = abi_decode_gmp_packet_data(&encoded).unwrap();

        assert_eq!(raw.sender, sender);
        assert_eq!(raw.receiver, receiver);
        assert!(raw.salt.is_empty());
        assert_eq!(raw.payload, payload);
    }

    #[test]
    fn test_abi_decode_gmp_packet_data_invalid_bytes() {
        assert!(abi_decode_gmp_packet_data(&[0xFF; 10]).is_err());
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_empty_instruction_data_rejected() {
        let encoded = AbiGmpSolanaPayload {
            packedAccounts: Vec::new().into(),
            instructionData: Vec::new().into(),
            payerPosition: 0,
        }
        .abi_encode_params();

        assert!(decode_abi_gmp_solana_payload(&encoded).is_err());
    }
}
