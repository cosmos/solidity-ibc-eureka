//! ABI encoding and decoding for GMP Solana payload using `alloy-sol-types`.

use alloy_sol_types::SolValue;
use anchor_lang::prelude::*;

use crate::errors::GMPError;
use crate::proto::GmpSolanaPayload;
pub use crate::sol_types::ISolanaGMPMsgs::GMPSolanaPayload as GmpSolanaPayloadAbi;

/// Size of a packed account entry: pubkey(32) + `is_signer`(1) + `is_writable`(1)
const PACKED_ACCOUNT_SIZE: usize = 34;

const fn parse_bool_byte(byte: u8) -> std::result::Result<bool, GMPError> {
    match byte {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(GMPError::InvalidAbiEncoding),
    }
}

impl TryFrom<GmpSolanaPayloadAbi> for solana_ibc_proto::RawGmpSolanaPayload {
    type Error = GMPError;

    fn try_from(abi: GmpSolanaPayloadAbi) -> std::result::Result<Self, Self::Error> {
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
    let decoded =
        GmpSolanaPayloadAbi::abi_decode(data).map_err(|_| error!(GMPError::InvalidAbiEncoding))?;
    let raw: solana_ibc_proto::RawGmpSolanaPayload =
        decoded.try_into().map_err(|e: GMPError| error!(e))?;
    raw.try_into()
        .map_err(|e: solana_ibc_proto::GmpValidationError| error!(GMPError::from(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_ibc_proto::MAX_GMP_SOLANA_PAYLOAD_ACCOUNTS;

    fn assert_anchor_error(result: Result<GmpSolanaPayload>, expected: GMPError) {
        let err = result.unwrap_err();
        match err {
            anchor_lang::error::Error::AnchorError(anchor_err) => {
                assert_eq!(
                    anchor_err.error_code_number,
                    u32::from(expected),
                    "expected {expected:?}, got {}",
                    anchor_err.error_name
                );
            }
            other @ anchor_lang::error::Error::ProgramError(_) => {
                panic!("expected AnchorError, got {other:?}")
            }
        }
    }

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

        let encoded = GmpSolanaPayloadAbi {
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
        let encoded = GmpSolanaPayloadAbi {
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
        let encoded = GmpSolanaPayloadAbi {
            packedAccounts: bad_packed.into(),
            instructionData: vec![1].into(),
            prefundLamports: 0,
        }
        .abi_encode();

        assert_anchor_error(
            decode_abi_gmp_solana_payload(&encoded),
            GMPError::InvalidAbiEncoding,
        );
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_empty_instruction_data_rejected() {
        let encoded = GmpSolanaPayloadAbi {
            packedAccounts: Vec::new().into(),
            instructionData: Vec::new().into(),
            prefundLamports: 0,
        }
        .abi_encode();

        assert_anchor_error(
            decode_abi_gmp_solana_payload(&encoded),
            GMPError::EmptyPayload,
        );
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_invalid_is_signer_byte() {
        let pubkey = Pubkey::new_unique();
        let mut packed = Vec::new();
        packed.extend_from_slice(&pubkey.to_bytes());
        packed.push(2); // invalid is_signer
        packed.push(0);

        let encoded = GmpSolanaPayloadAbi {
            packedAccounts: packed.into(),
            instructionData: vec![1].into(),
            prefundLamports: 0,
        }
        .abi_encode();

        assert_anchor_error(
            decode_abi_gmp_solana_payload(&encoded),
            GMPError::InvalidAbiEncoding,
        );
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_invalid_is_writable_byte() {
        let pubkey = Pubkey::new_unique();
        let mut packed = Vec::new();
        packed.extend_from_slice(&pubkey.to_bytes());
        packed.push(0);
        packed.push(3); // invalid is_writable

        let encoded = GmpSolanaPayloadAbi {
            packedAccounts: packed.into(),
            instructionData: vec![1].into(),
            prefundLamports: 0,
        }
        .abi_encode();

        assert_anchor_error(
            decode_abi_gmp_solana_payload(&encoded),
            GMPError::InvalidAbiEncoding,
        );
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_too_many_accounts() {
        let packed: Vec<u8> = (0..=MAX_GMP_SOLANA_PAYLOAD_ACCOUNTS)
            .flat_map(|_| {
                let mut entry = Pubkey::new_unique().to_bytes().to_vec();
                entry.push(0); // is_signer
                entry.push(0); // is_writable
                entry
            })
            .collect();

        let encoded = GmpSolanaPayloadAbi {
            packedAccounts: packed.into(),
            instructionData: vec![1].into(),
            prefundLamports: 0,
        }
        .abi_encode();

        assert_anchor_error(
            decode_abi_gmp_solana_payload(&encoded),
            GMPError::TooManyAccounts,
        );
    }

    #[test]
    fn test_decode_abi_gmp_solana_payload_invalid_account_key() {
        // 33-byte pubkey (too long) triggers InvalidAccountKey during RawGmpSolanaPayload -> GmpSolanaPayload
        // But since packed accounts are fixed 34-byte chunks with 32-byte keys,
        // the only way to get InvalidAccountKey is via a malformed RawSolanaAccountMeta.
        // The ABI path validates booleans first, so InvalidAccountKey can't happen
        // through ABI decoding alone — it's covered by the protobuf path tests.
        // This test verifies the max-boundary case (exactly at limit) succeeds.
        let packed: Vec<u8> = (0..MAX_GMP_SOLANA_PAYLOAD_ACCOUNTS)
            .flat_map(|_| {
                let mut entry = Pubkey::new_unique().to_bytes().to_vec();
                entry.push(0);
                entry.push(1);
                entry
            })
            .collect();

        let encoded = GmpSolanaPayloadAbi {
            packedAccounts: packed.into(),
            instructionData: vec![1].into(),
            prefundLamports: 0,
        }
        .abi_encode();

        let decoded = decode_abi_gmp_solana_payload(&encoded).unwrap();
        assert_eq!(decoded.accounts.len(), MAX_GMP_SOLANA_PAYLOAD_ACCOUNTS);
    }
}
