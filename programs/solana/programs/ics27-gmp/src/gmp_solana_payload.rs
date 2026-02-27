//! Decoding `GmpSolanaPayload` from ABI or protobuf encoding.

use anchor_lang::prelude::*;
use solana_ibc_proto::Protobuf;

use crate::errors::GMPError;
use crate::proto::GmpSolanaPayload;

/// Decode `GmpSolanaPayload` from either protobuf or ABI encoding based on the encoding string.
///
/// Used by `on_recv_packet` to extract the target program accounts and instruction data.
pub fn decode(value: &[u8], encoding: &str) -> Result<GmpSolanaPayload> {
    match encoding {
        crate::constants::ABI_ENCODING => {
            crate::abi::decode_abi_gmp_solana_payload(value).map_err(|e| {
                msg!("GMP ABI Solana payload decode failed: {}", e);
                error!(GMPError::InvalidSolanaPayload)
            })
        }
        crate::constants::ICS27_ENCODING => GmpSolanaPayload::decode(value).map_err(|e| {
            msg!("GMP Solana payload validation failed: {}", e);
            error!(GMPError::InvalidSolanaPayload)
        }),
        _ => Err(error!(GMPError::InvalidEncoding)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_ibc_proto::ProstMessage;

    #[test]
    fn test_decode_protobuf() {
        let pubkey = Pubkey::new_unique();
        let raw = solana_ibc_proto::RawGmpSolanaPayload {
            accounts: vec![solana_ibc_proto::RawSolanaAccountMeta {
                pubkey: pubkey.to_bytes().to_vec(),
                is_signer: true,
                is_writable: false,
            }],
            data: vec![0xAA, 0xBB],
            payer_position: Some(0),
        };
        let encoded = raw.encode_to_vec();

        let decoded = decode(&encoded, crate::constants::ICS27_ENCODING).unwrap();

        assert_eq!(decoded.accounts.len(), 1);
        assert_eq!(decoded.accounts[0].pubkey, pubkey);
        assert!(decoded.accounts[0].is_signer);
        assert!(!decoded.accounts[0].is_writable);
        assert_eq!(decoded.data, vec![0xAA, 0xBB]);
        assert_eq!(decoded.payer_position, Some(0));
    }

    #[test]
    fn test_decode_abi() {
        use crate::abi::*;
        use alloy_sol_types::SolValue;

        let pubkey = Pubkey::new_unique();
        let mut packed = Vec::new();
        packed.extend_from_slice(&pubkey.to_bytes());
        packed.push(1); // is_signer
        packed.push(0); // is_writable

        let encoded = AbiGmpSolanaPayload {
            packedAccounts: packed.into(),
            instructionData: vec![0xCC, 0xDD].into(),
            payerPosition: 0,
        }
        .abi_encode_params();

        let decoded = decode(&encoded, crate::constants::ABI_ENCODING).unwrap();

        assert_eq!(decoded.accounts.len(), 1);
        assert_eq!(decoded.accounts[0].pubkey, pubkey);
        assert!(decoded.accounts[0].is_signer);
        assert!(!decoded.accounts[0].is_writable);
        assert_eq!(decoded.data, vec![0xCC, 0xDD]);
        assert_eq!(decoded.payer_position, Some(0));
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
    fn test_decode_protobuf_empty_data_rejected() {
        let raw = solana_ibc_proto::RawGmpSolanaPayload {
            accounts: vec![],
            data: vec![],
            payer_position: None,
        };
        let encoded = raw.encode_to_vec();

        let result = decode(&encoded, crate::constants::ICS27_ENCODING);
        assert!(result.is_err());
    }
}
