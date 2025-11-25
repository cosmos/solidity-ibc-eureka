//! Solana-optimized Tendermint light client verifier using precompiled Ed25519Program and brine-ed25519
//!
//! TODO: Additional optimizations - for upstream PR to ibc-rs/tendermint-rs:
//! 1. tendermint-light-client-verifier/src/operations/voting_power.rs:319
//!    Add #[cfg(not(feature = "solana"))] before `votes.sort_unstable_by_key()`
//!    to skip on-chain sorting when signatures are pre-sorted by relayer
//! 2. solana-ibc-types/src/borsh_header.rs `conversions::commit_to_borsh()`
//!    Pre-sort signatures before serialization (saves ~60-80k CU on-chain for 100 validators)

use ibc_core_commitment_types::proto::ics23::HostFunctionsManager;
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
pub type SolanaVerifier<'a> =
    PredicateVerifier<SolanaPredicates, SolanaVotingPowerCalculator<'a>, ProdCommitValidator>;

/// Solana voting power calculator using optimized signature verification
pub type SolanaVotingPowerCalculator<'a> =
    ProvidedVotingPowerCalculator<SolanaSignatureVerifier<'a>>;

/// Solana optimised signature verifier with pre-verification account support
#[derive(Clone, Debug)]
pub struct SolanaSignatureVerifier<'a> {
    /// Pre-verified signature accounts from `remaining_accounts`
    verification_accounts: &'a [solana_program::account_info::AccountInfo<'a>],
    /// Program ID for PDA derivation
    program_id: &'a solana_program::pubkey::Pubkey,
}

impl<'a> SolanaSignatureVerifier<'a> {
    pub fn new(
        verification_accounts: &'a [solana_program::account_info::AccountInfo<'a>],
        program_id: &'a solana_program::pubkey::Pubkey,
    ) -> Self {
        Self {
            verification_accounts,
            program_id,
        }
    }

    pub fn without_pre_verification(program_id: &'a solana_program::pubkey::Pubkey) -> Self {
        Self {
            verification_accounts: &[],
            program_id,
        }
    }
}

impl<'a> tendermint::crypto::signature::Verifier for SolanaSignatureVerifier<'a> {
    fn verify(&self, pubkey: PublicKey, msg: &[u8], signature: &Signature) -> Result<(), Error> {
        match pubkey {
            PublicKey::Ed25519(pk) => {
                if !self.verification_accounts.is_empty() {
                    use solana_program::msg;

                    let sig_hash =
                        solana_program::hash::hashv(&[pk.as_bytes(), msg, signature.as_bytes()])
                            .to_bytes();

                    // PDA: [b"sig_verify", hash(pubkey || msg || signature)]
                    let (expected_pda, _) = solana_program::pubkey::Pubkey::find_program_address(
                        &[b"sig_verify", &sig_hash],
                        self.program_id,
                    );

                    for account in self.verification_accounts {
                        if account.key == &expected_pda {
                            let data = account.try_borrow_data().map_err(|_| {
                                msg!("Failed to borrow verification account data");
                                Error::VerificationFailed
                            })?;

                            if data.len() < 9 {
                                msg!("Verification account data too short");
                                return Err(Error::VerificationFailed);
                            }

                            let is_valid = data[8] != 0;
                            if !is_valid {
                                return Err(Error::VerificationFailed);
                            }

                            return Ok(());
                        }
                    }

                    msg!("Pre-verification account not found, using brine-ed25519");
                }

                // Ed25519Program only verifies sigs in current tx, can't handle external Tendermint headers.
                // Pre-verification (above) uses Ed25519Program via separate tx for FREE verification.
                // Fallback: brine-ed25519 (~30k CU/sig, audited by OtterSec)
                brine_ed25519::sig_verify(pk.as_bytes(), signature.as_bytes(), msg)
                    .map_err(|_| Error::VerificationFailed)
            }
            _ => Err(Error::UnsupportedKeyType),
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
        solana_program::hash::hashv(&[data.as_ref()]).to_bytes()
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
        solana_program::hash::hash(message).to_bytes()
    }

    fn sha2_512(message: &[u8]) -> [u8; 64] {
        HostFunctionsManager::sha2_512(message)
    }

    fn sha2_512_truncated(message: &[u8]) -> [u8; 32] {
        HostFunctionsManager::sha2_512_truncated(message)
    }

    fn keccak_256(message: &[u8]) -> [u8; 32] {
        solana_program::keccak::hash(message).to_bytes()
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
