//! ABI encoding and decoding for GMP Solana payload using `alloy-sol-types`.

use alloy_sol_types::SolValue;
use anchor_lang::prelude::*;

use crate::errors::GMPError;
use crate::proto::GmpSolanaPayload;

/// Size of a packed account entry: pubkey(32) + `is_signer`(1) + `is_writable`(1)
const PACKED_ACCOUNT_SIZE: usize = 34;

const fn parse_bool_byte(byte: u8) -> std::result::Result<bool, GMPError> {
    match byte {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(GMPError::InvalidAbiEncoding),
    }
}

impl TryFrom<crate::sol_types::ISolanaGMPMsgs::GMPSolanaPayload>
    for solana_ibc_proto::RawGmpSolanaPayload
{
    type Error = GMPError;

    fn try_from(
        abi: crate::sol_types::ISolanaGMPMsgs::GMPSolanaPayload,
    ) -> std::result::Result<Self, Self::Error> {
        let chunks = abi.packedAccounts.chunks_exact(PACKED_ACCOUNT_SIZE);
        if !chunks.remainder().is_empty() {
            return Err(GMPError::InvalidAbiEncoding);
        }
        let accounts = chunks
            .map(|chunk| {
                Ok(solana_ibc_proto::RawSolanaAccountMeta {
                    pubkey: chunk[..32].to_vec(),
                    is_signer: parse_bool_byte(chunk[32])?,
                    is_writable: parse_bool_byte(chunk[33])?,
                })
            })
            .collect::<std::result::Result<Vec<_>, GMPError>>()?;
        Ok(Self {
            accounts,
            data: abi.instructionData.into(),
            prefund_lamports: abi.prefundLamports,
        })
    }
}

/// Decode ABI-encoded [`GMPSolanaPayload`].
pub fn decode_abi_gmp_solana_payload(data: &[u8]) -> Result<GmpSolanaPayload> {
    let decoded = crate::sol_types::ISolanaGMPMsgs::GMPSolanaPayload::abi_decode(data)
        .map_err(|_| error!(GMPError::InvalidAbiEncoding))?;
    let raw: solana_ibc_proto::RawGmpSolanaPayload =
        decoded.try_into().map_err(|e: GMPError| error!(e))?;
    raw.try_into()
        .map_err(|_| error!(GMPError::InvalidSolanaPayload))
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

        let encoded = crate::sol_types::ISolanaGMPMsgs::GMPSolanaPayload {
            packedAccounts: packed.into(),
            instructionData: instr_data.clone().into(),
            prefundLamports: 8,
        }
        .abi_encode();

        let decoded = decode_abi_gmp_solana_payload(&encoded).unwrap();

        assert_eq!(decoded.accounts.len(), 2);
        assert_eq!(decoded.accounts[0].pubkey, pubkey1);
        assert!(decoded.accounts[0].is_signer);
        assert!(!decoded.accounts[0].is_writable);
        assert_eq!(decoded.accounts[1].pubkey, pubkey2);
        assert!(!decoded.accounts[1].is_signer);
        assert!(decoded.accounts[1].is_writable);
        assert_eq!(decoded.data, instr_data);
        assert_eq!(decoded.prefund_lamports, 8);
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_empty_accounts() {
        let instr_data = vec![1, 2, 3];
        let encoded = crate::sol_types::ISolanaGMPMsgs::GMPSolanaPayload {
            packedAccounts: Vec::new().into(),
            instructionData: instr_data.clone().into(),
            prefundLamports: 0,
        }
        .abi_encode();

        let decoded = decode_abi_gmp_solana_payload(&encoded).unwrap();

        assert!(decoded.accounts.is_empty());
        assert_eq!(decoded.data, instr_data);
        assert_eq!(decoded.prefund_lamports, 0);
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_too_short() {
        let data = vec![0u8; 95]; // less than 3 words
        assert!(decode_abi_gmp_solana_payload(&data).is_err());
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_misaligned_accounts() {
        let bad_packed = vec![0u8; 35]; // not a multiple of 34
        let encoded = crate::sol_types::ISolanaGMPMsgs::GMPSolanaPayload {
            packedAccounts: bad_packed.into(),
            instructionData: vec![1].into(),
            prefundLamports: 0,
        }
        .abi_encode();

        assert!(decode_abi_gmp_solana_payload(&encoded).is_err());
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_empty_instruction_data_rejected() {
        let encoded = crate::sol_types::ISolanaGMPMsgs::GMPSolanaPayload {
            packedAccounts: Vec::new().into(),
            instructionData: Vec::new().into(),
            prefundLamports: 0,
        }
        .abi_encode();

        assert!(decode_abi_gmp_solana_payload(&encoded).is_err());
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_invalid_is_signer_byte() {
        let pubkey = Pubkey::new_unique();
        let mut packed = Vec::new();
        packed.extend_from_slice(&pubkey.to_bytes());
        packed.push(2); // invalid is_signer
        packed.push(0);

        let encoded = crate::sol_types::ISolanaGMPMsgs::GMPSolanaPayload {
            packedAccounts: packed.into(),
            instructionData: vec![1].into(),
            prefundLamports: 0,
        }
        .abi_encode();

        assert!(decode_abi_gmp_solana_payload(&encoded).is_err());
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_invalid_is_writable_byte() {
        let pubkey = Pubkey::new_unique();
        let mut packed = Vec::new();
        packed.extend_from_slice(&pubkey.to_bytes());
        packed.push(0);
        packed.push(3); // invalid is_writable

        let encoded = crate::sol_types::ISolanaGMPMsgs::GMPSolanaPayload {
            packedAccounts: packed.into(),
            instructionData: vec![1].into(),
            prefundLamports: 0,
        }
        .abi_encode();

        assert!(decode_abi_gmp_solana_payload(&encoded).is_err());
    }
}
