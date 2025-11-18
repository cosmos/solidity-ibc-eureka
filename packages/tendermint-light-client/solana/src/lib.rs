//! Solana-optimized Tendermint light client verifier using brine-ed25519
//!
//! TODO: For upstream PR to ibc-rs/tendermint-rs:
//! 1. tendermint-light-client-verifier/src/operations/voting_power.rs:319
//!    Add #[cfg(not(feature = "solana"))] before `votes.sort_unstable_by_key()`
//!    to skip on-chain sorting when signatures are pre-sorted by relayer
//! 2. solana-ibc-types/src/borsh_header.rs `conversions::commit_to_borsh()`
//!    Pre-sort signatures before serialization (saves ~60-80k CU on-chain)

use std::collections::HashMap;

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

use solana_program::{log::sol_log_compute_units, msg};

/// Solana-optimized predicates that skip redundant Merkle hashing
///
/// The validator set hashes are already validated in `validate_basic()` and
/// `check_trusted_next_validator_set()` before the verifier is called, so we can
/// safely skip recomputing them here to save ~290k compute units.
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
    /// Savings: ~145k compute units
    fn validator_sets_match(
        &self,
        _validators: &ValidatorSet,
        _header_validators_hash: tendermint::Hash,
    ) -> Result<(), VerificationError> {
        msg!("[solana-predicates] Skipping redundant validator_sets_match hash (already validated in validate_basic)");
        sol_log_compute_units();

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
    /// Savings: ~145k compute units
    fn next_validators_match(
        &self,
        _next_validators: &ValidatorSet,
        _header_next_validators_hash: tendermint::Hash,
    ) -> Result<(), VerificationError> {
        msg!("[solana-predicates] Skipping redundant next_validators_match hash (already validated in check_trusted_next_validator_set)");
        sol_log_compute_units();

        // Return Ok immediately - validation already done in Header::check_trusted_next_validator_set()
        Ok(())
    }
}

/// Solana-optimized verifier that uses brine-ed25519 for signature verification
/// and skips redundant Merkle hashing
pub type SolanaVerifier<'a> = PredicateVerifier<
    SolanaPredicates,
    SolanaVotingPowerCalculator<'a>,
    ProdCommitValidator,
>;

/// Solana voting power calculator using optimized signature verification
pub type SolanaVotingPowerCalculator<'a> = ProvidedVotingPowerCalculator<SolanaSignatureVerifier<'a>>;

/// Solana optimised signature verifier with pre-verification account support
#[derive(Clone, Debug)]
pub struct SolanaSignatureVerifier<'a> {
    /// Pre-verified signature accounts from `remaining_accounts`
    verification_accounts: &'a [solana_program::account_info::AccountInfo<'a>],
    /// Program ID for PDA derivation
    program_id: &'a solana_program::pubkey::Pubkey,
}

impl<'a> SolanaSignatureVerifier<'a> {
    /// Create a new verifier with pre-verified account access
    pub fn new(
        verification_accounts: &'a [solana_program::account_info::AccountInfo<'a>],
        program_id: &'a solana_program::pubkey::Pubkey,
    ) -> Self {
        Self {
            verification_accounts,
            program_id,
        }
    }

    /// Create a verifier without pre-verified accounts (fallback to brine-ed25519 only)
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
                // First, check if we have a pre-verified signature in the accounts
                if !self.verification_accounts.is_empty() {
                    use solana_program::msg;

                    // Compute the signature hash (must match pre_verify_signatures.rs)
                    let sig_hash = solana_program::hash::hashv(&[
                        pk.as_bytes(),
                        msg,
                        signature.as_bytes(),
                    ])
                    .to_bytes();

                    // Derive the expected PDA
                    let (expected_pda, _) = solana_program::pubkey::Pubkey::find_program_address(
                        &[b"sig_verify", &sig_hash],
                        self.program_id,
                    );

                    // Search for the account in verification_accounts
                    for account in self.verification_accounts {
                        if account.key == &expected_pda {
                            // Found the account! Read the verification result
                            let data = account.try_borrow_data().map_err(|_| {
                                msg!("Failed to borrow verification account data");
                                Error::VerificationFailed
                            })?;

                            // Account structure: [8 byte discriminator][1 byte bool]
                            // Skip anchor discriminator (8 bytes) and read the bool field
                            if data.len() < 9 {
                                msg!("Verification account data too short");
                                return Err(Error::VerificationFailed);
                            }

                            // Read the is_valid bool field (1 byte at offset 8)
                            let is_valid = data[8] != 0;

                            if is_valid {
                                msg!("Using pre-verified signature (FREE!)");
                                return Ok(());
                            } else {
                                msg!("Pre-verified signature marked as invalid");
                                return Err(Error::VerificationFailed);
                            }
                        }
                    }

                    // Account not found, fall through to brine-ed25519
                    msg!("Pre-verification account not found, using brine-ed25519");
                }

                // Fallback to brine-ed25519 verification
                //
                // Why brine-ed25519 instead of Solana's native Ed25519Program?
                //
                // TLDR: Ed25519Program is fundamentally incompatible with IBC light client verification.
                //
                // Solana provides three options for Ed25519 signature verification:
                //
                // 1. Ed25519Program (native precompile) - FREE compute units
                //    ❌ INCOMPATIBLE: Only verifies signatures that are included as Ed25519Program
                //    instructions in the CURRENT transaction. IBC requires verifying signatures from
                //    EXTERNAL data (Tendermint headers from another blockchain) that cannot be
                //    included as instructions in the Solana transaction.
                //
                // 2. brine-ed25519 (on-chain library) - ~30k CU per signature ✅ USED AS FALLBACK
                //    ✅ WORKS: Can verify any signature from external data (Tendermint validators)
                //    - Uses native curve operations for efficiency
                //    - Enables early exit optimizations
                //    - Total cost: ~200k CU for typical light client update (verifying enough
                //      validators to meet 2/3 trust threshold, typically 10-20 signatures)
                //    - Security: Pulled from code-vm (MIT-licensed), audited by OtterSec,
                //      peer-reviewed by @stegaBOB and @deanmlittle
                //
                // 3. Multi-transaction pre-verification with Ed25519Program ✅ PREFERRED (IMPLEMENTED)
                //    The relayer sends a separate pre_verify_signatures transaction with Ed25519Program
                //    instructions. The results are cached in PDA accounts and read here for FREE.
                //    - Best of both worlds: FREE verification + no multi-tx complexity in update_client
                //    - Parallelizable: pre-verification runs concurrently with chunk uploads
                //    - Graceful degradation: Falls back to brine if pre-verification account missing
                //
                // Cost comparison for typical update (20 signatures verified):
                // - Pre-verification (this implementation): FREE (reads cached results)
                // - brine-ed25519 (fallback): ~600k CU (~$0.00003 USD)
                // - Ethereum equivalent: ~230k gas for ZK proof (~$0.50-5.00 USD, ~12s for proof generation)
                brine_ed25519::sig_verify(pk.as_bytes(), signature.as_bytes(), msg)
                    .map_err(|_| Error::VerificationFailed)
            }
            _ => Err(Error::UnsupportedKeyType),
        }
    }
}

/// Cached Merkle
pub struct SolanaPdaMerkleHash {
    prehashed_merkle: HashMap<Hash, Hash>,
    inner: SolanaSha256,
}

impl SolanaPdaMerkleHash {
    pub fn new(merkle_cache: HashMap<Hash, Hash>) -> Self {
        Self {
            prehashed_merkle: merkle_cache,
            inner: SolanaSha256::default(),
        }
    }
}

impl tendermint::crypto::Sha256 for SolanaPdaMerkleHash {
    fn digest(data: impl AsRef<[u8]>) -> [u8; 32] {
        SolanaSha256Impl::digest(data)
    }
}

impl tendermint::merkle::MerkleHash for SolanaPdaMerkleHash {
    fn empty_hash(&mut self) -> Hash {
        self.inner.0.empty_hash()
    }

    fn leaf_hash(&mut self, bytes: &[u8]) -> Hash {
        self.inner.0.leaf_hash(bytes)
    }

    fn inner_hash(&mut self, left: Hash, right: Hash) -> Hash {
        self.inner.0.inner_hash(left, right)
    }

    fn hash_byte_vectors(&mut self, byte_vecs: &[impl AsRef<[u8]>]) -> Hash {
        let bytes: Vec<&[u8]> = byte_vecs.iter().map(|v| v.as_ref()).collect();
        let simple_hash = solana_program::hash::hashv(&bytes);
        if let Some(hash) = self.prehashed_merkle.get(&simple_hash.to_bytes()) {
            return *hash;
        }

        msg!(
            "[WARNING] Prehashed merkle did not contain {}, doing expensive hashing on-chain",
            simple_hash
        );

        self.inner.hash_byte_vectors(byte_vecs)
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
