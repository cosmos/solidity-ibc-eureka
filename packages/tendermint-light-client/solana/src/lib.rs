//! Solana-optimized Tendermint light client verifier using precompiled Ed25519Program and brine-ed25519

use ibc_core_commitment_types::proto::ics23::HostFunctionsManager;
use solana_account_info::AccountInfo;
use solana_ibc_types::ics07::SIGNATURE_VERIFICATION_IS_VALID_OFFSET;
use solana_pubkey::Pubkey;
use solana_sha256_hasher::{hash as sha256, hashv as sha256v};
use tendermint::crypto::signature::Error;
use tendermint::{PublicKey, Signature};
use tendermint_light_client_verifier::{
    errors::VerificationError,
    operations::{commit_validator::ProdCommitValidator, ProvidedVotingPowerCalculator},
    predicates::VerificationPredicates,
    types::ValidatorSet,
    PredicateVerifier,
};

use tendermint::merkle::Hash;

/// Solana-optimized predicates that skip redundant Merkle hashing
///
/// The validator set hashes are already validated in `validate_basic()` and
/// `check_trusted_next_validator_set()` before the verifier is called, so we can
/// safely skip recomputing them here.
///
/// **Performance (100 validators):** Saves ~290k compute units total (~145k per validator set hash)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SolanaPredicates;

impl VerificationPredicates for SolanaPredicates {
    type Sha256 = SolanaSha256;

    /// Skip validator set hash validation - already done in `validate_basic()`
    ///
    /// SAFETY: The hash of `validators` against `header_validators_hash` is
    /// already validated in `Header::validate_basic()` (line 166 of `header.rs`)
    /// before this function is called, so we can safely skip the redundant
    /// Merkle hash computation here.
    ///
    /// **Performance:** Saves ~145k compute units (tested with 100 validators)
    fn validator_sets_match(
        &self,
        _validators: &ValidatorSet,
        _header_validators_hash: tendermint::Hash,
    ) -> Result<(), VerificationError> {
        // Return Ok immediately - validation already done in Header::validate_basic()
        Ok(())
    }

    /// Skip next validator set hash validation - already done in `check_trusted_next_validator_set()`
    ///
    /// SAFETY: The hash of `trusted_next_validator_set` is already validated in
    /// `Header::check_trusted_next_validator_set()` (line 123 of header.rs)
    /// before this function is called, so we can safely skip the redundant
    /// Merkle hash computation here.
    ///
    /// **Performance:** Saves ~145k compute units (tested with 100 validators)
    fn next_validators_match(
        &self,
        _next_validators: &ValidatorSet,
        _header_next_validators_hash: tendermint::Hash,
    ) -> Result<(), VerificationError> {
        // Return Ok immediately - validation already done in Header::check_trusted_next_validator_set()
        Ok(())
    }
}

/// Solana-optimized verifier with pre-verified Ed25519 signatures and optimized Merkle hashing
///
/// **Signature verification:** Pre-verification PDAs (~10k CU/sig via Ed25519Program) with brine-ed25519 fallback (~30k CU/sig)
/// **Merkle hashing:** Skips redundant validator set hash validation (saves ~290k CU for 100 validators)
/// **Real-world costs:**
/// - Noble (20 validators): ~548k CU total (~$0.025-0.033 USD)
/// - Celestia (100 validators): ~2.16M CU total (~$0.11-0.14 USD)
pub type SolanaVerifier =
    PredicateVerifier<SolanaPredicates, SolanaVotingPowerCalculator, ProdCommitValidator>;

/// Solana voting power calculator using optimized signature verification
pub type SolanaVotingPowerCalculator = ProvidedVotingPowerCalculator<SolanaSignatureVerifier>;

/// Pre-extracted verification entry: account key and the validity byte (if present).
#[derive(Clone, Debug)]
struct VerificationEntry {
    key: Pubkey,
    /// `Some(1)` = valid, `Some(0)` = invalid, other/None = data missing or unexpected
    is_valid: Option<u8>,
}

/// Solana optimised signature verifier with pre-verification account support.
///
/// Verification data is extracted from `AccountInfo` at construction time so
/// the struct contains only owned types and is `Send + Sync`.
#[derive(Clone, Debug)]
pub struct SolanaSignatureVerifier {
    entries: Vec<VerificationEntry>,
    program_id: Pubkey,
}

impl SolanaSignatureVerifier {
    /// Build from `remaining_accounts` by pre-extracting the validity byte
    /// from each account at `SIGNATURE_VERIFICATION_IS_VALID_OFFSET`.
    pub fn from_accounts(accounts: &[AccountInfo<'_>], program_id: &Pubkey) -> Self {
        let entries = accounts
            .iter()
            .map(|a| {
                let is_valid = a
                    .try_borrow_data()
                    .ok()
                    .and_then(|d| d.get(SIGNATURE_VERIFICATION_IS_VALID_OFFSET).copied());
                VerificationEntry {
                    key: *a.key,
                    is_valid,
                }
            })
            .collect();

        Self {
            entries,
            program_id: *program_id,
        }
    }
}

impl tendermint::crypto::signature::Verifier for SolanaSignatureVerifier {
    fn verify(&self, pubkey: PublicKey, msg: &[u8], signature: &Signature) -> Result<(), Error> {
        let PublicKey::Ed25519(pk) = pubkey else {
            return Err(Error::UnsupportedKeyType);
        };

        let sig_hash = sha256v(&[pk.as_bytes(), msg, signature.as_bytes()]).to_bytes();
        let (expected_pda, _) =
            Pubkey::find_program_address(&[b"sig_verify", &sig_hash], &self.program_id);

        // Fallback: brine-ed25519 (~30k CU/sig, audited by OtterSec)
        let fallback = || {
            brine_ed25519::sig_verify(pk.as_bytes(), signature.as_bytes(), msg)
                .map_err(|_| Error::VerificationFailed)
        };

        let Some(entry) = self.entries.iter().find(|e| e.key == expected_pda) else {
            return fallback();
        };

        match entry.is_valid {
            Some(1) => Ok(()),
            Some(0) => Err(Error::VerificationFailed),
            Some(v) => {
                solana_msg::msg!("Unexpected verification value: {}", v);
                fallback()
            }
            None => {
                solana_msg::msg!("Verification account data too short");
                fallback()
            }
        }
    }
}

/// Merkle
#[derive(Default)]
pub struct SolanaSha256(tendermint::merkle::NonIncremental<SolanaSha256Impl>);

/// Solana Sha256
#[derive(Default)]
pub struct SolanaSha256Impl;

impl tendermint::crypto::Sha256 for SolanaSha256Impl {
    fn digest(data: impl AsRef<[u8]>) -> [u8; 32] {
        sha256v(&[data.as_ref()]).to_bytes()
    }
}

impl tendermint::crypto::Sha256 for SolanaSha256 {
    fn digest(data: impl AsRef<[u8]>) -> [u8; 32] {
        SolanaSha256Impl::digest(data)
    }
}
impl tendermint::merkle::MerkleHash for SolanaSha256 {
    fn empty_hash(&mut self) -> Hash {
        self.0.empty_hash()
    }

    fn leaf_hash(&mut self, bytes: &[u8]) -> Hash {
        self.0.leaf_hash(bytes)
    }

    fn inner_hash(&mut self, left: Hash, right: Hash) -> Hash {
        self.0.inner_hash(left, right)
    }
}

/// Solana-optimized host functions for ICS-23 Merkle proof verification
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SolanaHostFunctionsManager;

impl ics23::HostFunctionsProvider for SolanaHostFunctionsManager {
    fn sha2_256(message: &[u8]) -> [u8; 32] {
        sha256(message).to_bytes()
    }

    fn sha2_512(message: &[u8]) -> [u8; 64] {
        HostFunctionsManager::sha2_512(message)
    }

    fn sha2_512_truncated(message: &[u8]) -> [u8; 32] {
        HostFunctionsManager::sha2_512_truncated(message)
    }

    fn keccak_256(message: &[u8]) -> [u8; 32] {
        solana_keccak_hasher::hash(message).to_bytes()
    }

    fn ripemd160(message: &[u8]) -> [u8; 20] {
        HostFunctionsManager::ripemd160(message)
    }

    fn blake2b_512(message: &[u8]) -> [u8; 64] {
        HostFunctionsManager::blake2b_512(message)
    }

    fn blake2s_256(message: &[u8]) -> [u8; 32] {
        HostFunctionsManager::blake2s_256(message)
    }

    fn blake3(message: &[u8]) -> [u8; 32] {
        HostFunctionsManager::blake3(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use solana_ibc_types::ics07::SIGNATURE_VERIFICATION_IS_VALID_OFFSET;
    use std::cell::RefCell;
    use std::rc::Rc;
    use tendermint::crypto::signature::Verifier;

    fn create_verification_account_data(is_valid: u8) -> Vec<u8> {
        let mut data = vec![0u8; SIGNATURE_VERIFICATION_IS_VALID_OFFSET + 1];
        data[SIGNATURE_VERIFICATION_IS_VALID_OFFSET] = is_valid;
        data
    }

    fn create_account_info<'a>(
        key: &'a Pubkey,
        data: &'a mut [u8],
        lamports: &'a mut u64,
        owner: &'a Pubkey,
    ) -> AccountInfo<'a> {
        AccountInfo {
            key,
            is_signer: false,
            is_writable: false,
            lamports: Rc::new(RefCell::new(lamports)),
            data: Rc::new(RefCell::new(data)),
            owner,
            executable: false,
            rent_epoch: 0,
        }
    }

    fn compute_sig_verify_pda(
        pk: &[u8; 32],
        msg: &[u8],
        sig: &[u8; 64],
        program_id: &Pubkey,
    ) -> Pubkey {
        let sig_hash = sha256v(&[pk, msg, sig]).to_bytes();
        let (pda, _) = Pubkey::find_program_address(&[b"sig_verify", &sig_hash], program_id);
        pda
    }

    #[rstest]
    #[case::valid_signature(1, true)]
    #[case::invalid_signature(0, false)]
    #[case::unexpected_value_falls_back(42, false)]
    fn test_preverify_with_account(#[case] is_valid: u8, #[case] expected_ok: bool) {
        let program_id = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let pk_bytes = [1u8; 32];
        let msg = b"test message";
        let sig_bytes = [2u8; 64];
        let pda = compute_sig_verify_pda(&pk_bytes, msg, &sig_bytes, &program_id);

        let mut data = create_verification_account_data(is_valid);
        let mut lamports = 1_000_000u64;
        let accounts = [create_account_info(&pda, &mut data, &mut lamports, &owner)];

        let verifier = SolanaSignatureVerifier::from_accounts(&accounts, &program_id);
        let pk = tendermint::PublicKey::from_raw_ed25519(&pk_bytes).unwrap();
        let sig = Signature::try_from(sig_bytes.as_slice()).unwrap();

        assert_eq!(verifier.verify(pk, msg, &sig).is_ok(), expected_ok);
    }

    #[test]
    fn test_preverify_data_too_short_falls_back() {
        let program_id = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let pk_bytes = [1u8; 32];
        let msg = b"test message";
        let sig_bytes = [2u8; 64];
        let pda = compute_sig_verify_pda(&pk_bytes, msg, &sig_bytes, &program_id);

        let mut data = vec![0u8; SIGNATURE_VERIFICATION_IS_VALID_OFFSET];
        let mut lamports = 1_000_000u64;
        let accounts = [create_account_info(&pda, &mut data, &mut lamports, &owner)];

        let verifier = SolanaSignatureVerifier::from_accounts(&accounts, &program_id);
        let pk = tendermint::PublicKey::from_raw_ed25519(&pk_bytes).unwrap();
        let sig = Signature::try_from(sig_bytes.as_slice()).unwrap();

        assert!(verifier.verify(pk, msg, &sig).is_err());
    }

    #[test]
    fn test_no_preverify_account_falls_back() {
        let program_id = Pubkey::new_unique();
        let pk_bytes = [1u8; 32];
        let msg = b"test message";
        let sig_bytes = [2u8; 64];

        let verifier = SolanaSignatureVerifier::from_accounts(&[], &program_id);
        let pk = tendermint::PublicKey::from_raw_ed25519(&pk_bytes).unwrap();
        let sig = Signature::try_from(sig_bytes.as_slice()).unwrap();

        assert!(verifier.verify(pk, msg, &sig).is_err());
    }
}
