//! ABI decoder for GMP packet data.
//!
//! Decodes ABI-encoded `GMPPacketData(string, string, bytes, bytes, string)`
//! from Ethereum's Solidity ABI encoding format.

use anchor_lang::prelude::*;
use solana_ibc_proto::Protobuf;

use crate::errors::GMPError;

const WORD_SIZE: usize = 32;
const NUM_FIELDS: usize = 5;
const HEAD_SIZE: usize = NUM_FIELDS * WORD_SIZE;

/// Decoded ABI GMP packet data (raw bytes, before constrained type validation).
pub struct AbiDecodedGmpPacket {
    pub sender: Vec<u8>,
    pub receiver: Vec<u8>,
    pub salt: Vec<u8>,
    pub payload: Vec<u8>,
    pub memo: Vec<u8>,
}

impl AbiDecodedGmpPacket {
    /// Convert raw decoded fields into a validated `GmpPacketData`.
    pub fn into_gmp_packet_data(self) -> Result<solana_ibc_proto::GmpPacketData> {
        let sender: solana_ibc_proto::Sender = core::str::from_utf8(&self.sender)
            .map_err(|_| error!(GMPError::InvalidAbiEncoding))?
            .to_string()
            .try_into()
            .map_err(|_| error!(GMPError::InvalidPacketData))?;

        let receiver: solana_ibc_proto::Receiver = core::str::from_utf8(&self.receiver)
            .map_err(|_| error!(GMPError::InvalidAbiEncoding))?
            .to_string()
            .try_into()
            .map_err(|_| error!(GMPError::InvalidPacketData))?;

        let salt: solana_ibc_proto::Salt = self
            .salt
            .try_into()
            .map_err(|_| error!(GMPError::InvalidPacketData))?;

        let payload: solana_ibc_proto::Payload = self
            .payload
            .try_into()
            .map_err(|_| error!(GMPError::InvalidPacketData))?;

        let memo: solana_ibc_proto::Memo = core::str::from_utf8(&self.memo)
            .map_err(|_| error!(GMPError::InvalidAbiEncoding))?
            .to_string()
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
}

/// Read a big-endian uint256 as usize from a 32-byte word.
///
/// Only the last 8 bytes are used; the upper 24 must be zero.
fn read_offset(data: &[u8], word_index: usize) -> Result<usize> {
    let start = word_index * WORD_SIZE;
    let end = start + WORD_SIZE;
    require!(end <= data.len(), GMPError::InvalidAbiEncoding);

    let upper = &data[start..start + 24];
    require!(upper.iter().all(|&b| b == 0), GMPError::InvalidAbiEncoding);

    let bytes: [u8; 8] = data[start + 24..end]
        .try_into()
        .map_err(|_| error!(GMPError::InvalidAbiEncoding))?;
    Ok(u64::from_be_bytes(bytes) as usize)
}

/// Read a dynamic `bytes`/`string` field from the ABI data at the given offset.
fn read_dynamic_bytes(data: &[u8], offset: usize) -> Result<Vec<u8>> {
    require!(
        offset + WORD_SIZE <= data.len(),
        GMPError::InvalidAbiEncoding
    );

    let len_bytes: [u8; 8] = data[offset + 24..offset + WORD_SIZE]
        .try_into()
        .map_err(|_| error!(GMPError::InvalidAbiEncoding))?;
    let len = u64::from_be_bytes(len_bytes) as usize;

    let data_start = offset + WORD_SIZE;
    require!(data_start + len <= data.len(), GMPError::InvalidAbiEncoding);

    Ok(data[data_start..data_start + len].to_vec())
}

/// Decode ABI-encoded `GMPPacketData(string, string, bytes, bytes, string)`.
///
/// Solidity's `abi.encode(struct)` wraps the struct in an outer tuple offset:
/// - `[0..32]`: outer offset word (always 0x20, pointing to byte 32)
/// - `[32..192]`: 5 x 32-byte field offset words (relative to byte 32)
/// - At each offset (relative to byte 32): 32-byte length word + padded data
pub fn decode_abi_gmp_packet(data: &[u8]) -> Result<AbiDecodedGmpPacket> {
    // Skip the outer tuple offset word (first 32 bytes)
    require!(
        data.len() >= WORD_SIZE + HEAD_SIZE,
        GMPError::InvalidAbiEncoding
    );
    let tuple_data = &data[WORD_SIZE..];

    let offset_sender = read_offset(tuple_data, 0)?;
    let offset_receiver = read_offset(tuple_data, 1)?;
    let offset_salt = read_offset(tuple_data, 2)?;
    let offset_payload = read_offset(tuple_data, 3)?;
    let offset_memo = read_offset(tuple_data, 4)?;

    Ok(AbiDecodedGmpPacket {
        sender: read_dynamic_bytes(tuple_data, offset_sender)?,
        receiver: read_dynamic_bytes(tuple_data, offset_receiver)?,
        salt: read_dynamic_bytes(tuple_data, offset_salt)?,
        payload: read_dynamic_bytes(tuple_data, offset_payload)?,
        memo: read_dynamic_bytes(tuple_data, offset_memo)?,
    })
}

/// Pad length up to the next 32-byte boundary.
const fn pad_to_32(len: usize) -> usize {
    len.div_ceil(32) * 32
}

/// Encode a dynamic ABI field (bytes/string): 32-byte length word + padded data.
fn encode_dynamic(data: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; 32];
    let len = data.len() as u64;
    result[24..32].copy_from_slice(&len.to_be_bytes());
    result.extend_from_slice(data);
    let padded_len = pad_to_32(data.len());
    result.resize(32 + padded_len, 0);
    result
}

/// Encode `GMPPacketData(string, string, bytes, bytes, string)` as ABI.
///
/// Produces the same layout as Solidity's `abi.encode(GMPPacketData{...})`:
/// - `[0..32]`: outer tuple offset (0x20)
/// - `[32..192]`: 5 field offset words
/// - dynamic data for each field
pub fn encode_abi_gmp_packet(
    sender: &str,
    receiver: &str,
    salt: &[u8],
    payload: &[u8],
    memo: &str,
) -> Vec<u8> {
    let fields: Vec<Vec<u8>> = vec![
        encode_dynamic(sender.as_bytes()),
        encode_dynamic(receiver.as_bytes()),
        encode_dynamic(salt),
        encode_dynamic(payload),
        encode_dynamic(memo.as_bytes()),
    ];

    let mut offsets = vec![0u8; HEAD_SIZE];
    let mut current_offset = HEAD_SIZE;
    for (i, field) in fields.iter().enumerate() {
        let offset = current_offset as u64;
        offsets[i * WORD_SIZE + 24..i * WORD_SIZE + 32].copy_from_slice(&offset.to_be_bytes());
        current_offset += field.len();
    }

    // Prepend outer tuple offset (0x20) to match abi.encode(struct)
    let mut outer_offset = vec![0u8; WORD_SIZE];
    outer_offset[24..32].copy_from_slice(&(WORD_SIZE as u64).to_be_bytes());

    let mut result = outer_offset;
    result.extend_from_slice(&offsets);
    for field in fields {
        result.extend_from_slice(&field);
    }
    result
}

/// Decode `GmpPacketData` from either protobuf or ABI encoding based on the encoding string.
///
/// Used by `on_ack_packet` and `on_timeout_packet` to extract sender from the original packet.
pub fn decode_gmp_packet_data(
    value: &[u8],
    encoding: &str,
) -> Result<solana_ibc_proto::GmpPacketData> {
    match encoding {
        crate::constants::ABI_ENCODING => decode_abi_gmp_packet(value)?
            .into_gmp_packet_data()
            .map_err(|e| {
                msg!("GMP ABI packet validation failed: {}", e);
                error!(GMPError::InvalidPacketData)
            }),
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
    fn test_decode_roundtrip() {
        let sender = "0x1234567890abcdef";
        let receiver = "So1ana1111111111111111111111111111111111111";
        let salt = b"test-salt";
        let payload = b"some payload data";
        let memo = "hello memo";

        let encoded = encode_abi_gmp_packet(sender, receiver, salt, payload, memo);
        let decoded = decode_abi_gmp_packet(&encoded).unwrap();

        assert_eq!(decoded.sender, sender.as_bytes());
        assert_eq!(decoded.receiver, receiver.as_bytes());
        assert_eq!(decoded.salt, salt);
        assert_eq!(decoded.payload, payload);
        assert_eq!(decoded.memo, memo.as_bytes());
    }

    #[test]
    fn test_decode_empty_fields() {
        let encoded = encode_abi_gmp_packet("sender", "", &[], &[1], "");
        let decoded = decode_abi_gmp_packet(&encoded).unwrap();

        assert_eq!(decoded.sender, b"sender");
        assert_eq!(decoded.receiver, b"");
        assert_eq!(decoded.salt, &[] as &[u8]);
        assert_eq!(decoded.payload, &[1]);
        assert_eq!(decoded.memo, b"");
    }

    #[test]
    fn test_decode_too_short() {
        let data = vec![0u8; WORD_SIZE + HEAD_SIZE - 1];
        assert!(decode_abi_gmp_packet(&data).is_err());
    }

    #[test]
    fn test_into_gmp_packet_data() {
        let sender = "cosmos1sender";
        let receiver = "11111111111111111111111111111111"; // 32-char base58 pubkey
        let payload = vec![1, 2, 3, 4];

        let encoded = encode_abi_gmp_packet(sender, receiver, &[], &payload, "");
        let decoded = decode_abi_gmp_packet(&encoded).unwrap();
        let packet = decoded.into_gmp_packet_data().unwrap();

        assert_eq!(&*packet.sender, sender);
        assert_eq!(&*packet.receiver, receiver);
        assert!(packet.salt.is_empty());
        assert_eq!(&*packet.payload, &payload);
    }
}
