//! ABI encoding and decoding for GMP packet data using `alloy-sol-types`.
//!
//! Decodes/encodes ABI-encoded `GMPPacketData(string, string, bytes, bytes, string)`
//! and `GmpSolanaPayload(bytes, bytes, uint32)` from Ethereum's Solidity ABI format.

use alloy_sol_types::SolValue;
use anchor_lang::prelude::*;
use solana_ibc_proto::Protobuf;

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

/// Decode ABI-encoded `GmpSolanaPayload(bytes, bytes, uint32)`.
///
/// Uses `abi_decode_params` because Solidity encodes this with
/// `abi.encode(bytes, bytes, uint32)` (three separate params without an outer tuple offset).
pub fn decode_abi_gmp_solana_payload(data: &[u8]) -> Result<GmpSolanaPayload> {
    let decoded = AbiGmpSolanaPayload::abi_decode_params(data)
        .map_err(|_| error!(GMPError::InvalidAbiEncoding))?;

    let packed_bytes = &decoded.packedAccounts;
    require!(
        packed_bytes.len() % PACKED_ACCOUNT_SIZE == 0,
        GMPError::InvalidAbiEncoding
    );

    let accounts = packed_bytes
        .chunks_exact(PACKED_ACCOUNT_SIZE)
        .map(|chunk| {
            let pubkey_bytes: [u8; 32] = chunk[..32]
                .try_into()
                .map_err(|_| error!(GMPError::InvalidAbiEncoding))?;
            Ok(solana_ibc_proto::SolanaAccountMeta {
                pubkey: Pubkey::from(pubkey_bytes),
                is_signer: chunk[32] != 0,
                is_writable: chunk[33] != 0,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(GmpSolanaPayload {
        data: decoded.instructionData.into(),
        accounts,
        payer_position: Some(decoded.payerPosition),
    })
}

/// Encode `GMPPacketData(string, string, bytes, bytes, string)` as ABI.
///
/// Produces the same layout as Solidity's `abi.encode(GMPPacketData{...})`.
pub fn encode_abi_gmp_packet(
    sender: &str,
    receiver: &str,
    salt: &[u8],
    payload: &[u8],
    memo: &str,
) -> Vec<u8> {
    AbiGmpPacketData {
        sender: sender.into(),
        receiver: receiver.into(),
        salt: salt.to_vec().into(),
        payload: payload.to_vec().into(),
        memo: memo.into(),
    }
    .abi_encode()
}

/// Decode `GmpPacketData` from either protobuf or ABI encoding based on the encoding string.
///
/// Used by `on_recv_packet`, `on_ack_packet` and `on_timeout_packet` to extract packet fields.
pub fn decode_gmp_packet_data(
    value: &[u8],
    encoding: &str,
) -> Result<solana_ibc_proto::GmpPacketData> {
    match encoding {
        crate::constants::ABI_ENCODING => {
            let decoded = AbiGmpPacketData::abi_decode(value)
                .map_err(|_| error!(GMPError::InvalidAbiEncoding))?;

            let sender: solana_ibc_proto::Sender = decoded
                .sender
                .try_into()
                .map_err(|_| error!(GMPError::InvalidPacketData))?;

            let receiver: solana_ibc_proto::Receiver = decoded
                .receiver
                .try_into()
                .map_err(|_| error!(GMPError::InvalidPacketData))?;

            let salt: solana_ibc_proto::Salt = decoded
                .salt
                .to_vec()
                .try_into()
                .map_err(|_| error!(GMPError::InvalidPacketData))?;

            let payload: solana_ibc_proto::Payload = decoded
                .payload
                .to_vec()
                .try_into()
                .map_err(|_| error!(GMPError::InvalidPacketData))?;

            let memo: solana_ibc_proto::Memo = decoded
                .memo
                .try_into()
                .map_err(|_| error!(GMPError::InvalidPacketData))?;

            Ok(solana_ibc_proto::GmpPacketData {
                sender,
                receiver,
                salt,
                payload,
                memo,
            })
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

    #[test]
    fn test_encode_decode_roundtrip() {
        let sender = "0x1234567890abcdef";
        let receiver = "So1ana1111111111111111111111111111111111111";
        let salt = b"test-salt";
        let payload = b"some payload data";
        let memo = "hello memo";

        let encoded = encode_abi_gmp_packet(sender, receiver, salt, payload, memo);
        let decoded =
            AbiGmpPacketData::abi_decode(&encoded).expect("failed to decode ABI packet");

        assert_eq!(decoded.sender, sender);
        assert_eq!(decoded.receiver, receiver);
        assert_eq!(decoded.salt.as_ref(), salt);
        assert_eq!(decoded.payload.as_ref(), payload);
        assert_eq!(decoded.memo, memo);
    }

    #[test]
    fn test_encode_decode_empty_fields() {
        let encoded = encode_abi_gmp_packet("sender", "", &[], &[1], "");
        let decoded =
            AbiGmpPacketData::abi_decode(&encoded).expect("failed to decode ABI packet");

        assert_eq!(decoded.sender, "sender");
        assert_eq!(decoded.receiver, "");
        assert!(decoded.salt.is_empty());
        assert_eq!(decoded.payload.as_ref(), &[1]);
        assert_eq!(decoded.memo, "");
    }

    #[test]
    fn test_decode_too_short() {
        assert!(AbiGmpPacketData::abi_decode(&[0u8; 31]).is_err());
    }

    #[test]
    fn test_decode_gmp_packet_data_abi() {
        let sender = "cosmos1sender";
        let receiver = "11111111111111111111111111111111"; // 32-char base58 pubkey
        let payload = vec![1, 2, 3, 4];

        let encoded = encode_abi_gmp_packet(sender, receiver, &[], &payload, "");
        let packet = decode_gmp_packet_data(&encoded, crate::constants::ABI_ENCODING).unwrap();

        assert_eq!(&*packet.sender, sender);
        assert_eq!(&*packet.receiver, receiver);
        assert!(packet.salt.is_empty());
        assert_eq!(&*packet.payload, &payload);
    }
}
