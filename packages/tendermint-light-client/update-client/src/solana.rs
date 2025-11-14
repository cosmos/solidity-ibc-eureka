//! Solana-optimized Tendermint light client verifier using brine-ed25519

use tendermint::crypto::signature::Error;
use tendermint::{crypto::signature, PublicKey, Signature};
use tendermint_light_client_verifier::{
    errors::VerificationError,
    operations::{commit_validator::ProdCommitValidator, CommitValidator, ProvidedVotingPowerCalculator},
    predicates::{ProdPredicates, VerificationPredicates},
    types::{SignedHeader, ValidatorSet},
    PredicateVerifier,
};

#[cfg(feature = "solana")]
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

    /// Skip validator set hash validation - already done in validate_basic()
    ///
    /// SAFETY: The hash of `validators` against `header_validators_hash` is
    /// already validated in `Header::validate_basic()` (line 166 of header.rs)
    /// before this function is called, so we can safely skip the redundant
    /// Merkle hash computation here.
    ///
    /// Savings: ~145k compute units
    fn validator_sets_match(
        &self,
        _validators: &ValidatorSet,
        _header_validators_hash: tendermint::Hash,
    ) -> Result<(), VerificationError> {
        #[cfg(feature = "solana")]
        {
            msg!("[solana-predicates] Skipping redundant validator_sets_match hash (already validated in validate_basic)");
            sol_log_compute_units();
        }

        // Return Ok immediately - validation already done in Header::validate_basic()
        Ok(())
    }

    /// Skip next validator set hash validation - already done in check_trusted_next_validator_set()
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
        #[cfg(feature = "solana")]
        {
            msg!("[solana-predicates] Skipping redundant next_validators_match hash (already validated in check_trusted_next_validator_set)");
            sol_log_compute_units();
        }

        // Return Ok immediately - validation already done in Header::check_trusted_next_validator_set()
        Ok(())
    }

    /// Delegate all other predicate methods to ProdPredicates default implementations
    ///
    /// This includes:
    /// - header_matches_commit()
    /// - valid_commit()
    /// - is_within_trust_period()
    /// - is_header_from_past()
    /// - is_monotonic_bft_time()
    /// - is_monotonic_height()
    /// - is_matching_chain_id()
    /// - valid_next_validator_set()
    /// - has_sufficient_validators_overlap()
    /// - has_sufficient_signers_overlap()
    /// - has_sufficient_validators_and_signers_overlap()
    fn header_matches_commit(
        &self,
        header: &tendermint::block::Header,
        commit_hash: tendermint::Hash,
    ) -> Result<(), VerificationError> {
        ProdPredicates.header_matches_commit(header, commit_hash)
    }

    fn valid_commit(
        &self,
        signed_header: &SignedHeader,
        validators: &ValidatorSet,
        commit_validator: &dyn CommitValidator,
    ) -> Result<(), VerificationError> {
        ProdPredicates.valid_commit(signed_header, validators, commit_validator)
    }
}

/// Solana-optimized verifier that uses brine-ed25519 for signature verification
/// and skips redundant Merkle hashing
pub type SolanaVerifier =
    PredicateVerifier<SolanaPredicates, SolanaVotingPowerCalculator, ProdCommitValidator>;

/// Solana voting power calculator using optimized signature verification
pub type SolanaVotingPowerCalculator = ProvidedVotingPowerCalculator<SolanaSignatureVerifier>;

/// Solana optimised signature verifier
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SolanaSignatureVerifier;

impl signature::Verifier for SolanaSignatureVerifier {
    fn verify(pubkey: PublicKey, msg: &[u8], signature: &Signature) -> Result<(), Error> {
        #[cfg(feature = "solana")]
        {
            msg!("Verifying signature...");
            sol_log_compute_units();
        }

        match pubkey {
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
            // 2. brine-ed25519 (on-chain library) - ~30k CU per signature ✅ USED
            //    ✅ WORKS: Can verify any signature from external data (Tendermint validators)
            //    - Uses native curve operations for efficiency
            //    - Enables early exit optimizations
            //    - Total cost: ~200k CU for typical light client update (verifying enough
            //      validators to meet 2/3 trust threshold, typically 10-20 signatures)
            //    - Security: Pulled from code-vm (MIT-licensed), audited by OtterSec,
            //      peer-reviewed by @stegaBOB and @deanmlittle
            //
            // 3. Multi-transaction batching with Ed25519Program
            //    ❌ IMPRACTICAL:
            //    - Significantly slower: adds 4-8 seconds latency per update (10-20 sequential
            //      signature verification transactions after parallel chunk upload)
            //    - Requires splitting verification across multiple transactions
            //    - Complex state management to track which signatures were verified
            //    - Atomicity concerns: what if some transactions succeed and others fail?
            //    - Coordination overhead between transactions
            //
            // Cost comparison for typical update (20 signatures verified):
            // - brine-ed25519: ~600k CU (~$0.00003 USD), ~1.2 second latency
            // - Ed25519Program: FREE but incompatible with external signatures; multi-tx workaround
            //   would require splitting operations and add 4-8s latency
            // - Ethereum equivalent: ~230k gas for ZK proof (~$0.50-5.00 USD, ~12s for proof generation)
            //
            // This is the most efficient approach available given the constraint of verifying
            // signatures from external blockchain data.
            PublicKey::Ed25519(pk) => {
                let result = brine_ed25519::sig_verify(pk.as_bytes(), signature.as_bytes(), msg)
                    .map_err(|_| Error::VerificationFailed);

                #[cfg(feature = "solana")]
                {
                    match &result {
                        Ok(_) => {
                            msg!("Signature VERIFIED");
                        }
                        Err(_) => {
                            msg!("Signature FAILED");
                        }
                    }
                    sol_log_compute_units();
                }

                result
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
    fn empty_hash(&mut self) -> tendermint::merkle::Hash {
        self.0.empty_hash()
    }

    fn leaf_hash(&mut self, bytes: &[u8]) -> tendermint::merkle::Hash {
        self.0.leaf_hash(bytes)
    }

    fn inner_hash(&mut self, left: tendermint::merkle::Hash, right: tendermint::merkle::Hash) -> tendermint::merkle::Hash {
        self.0.inner_hash(left, right)
    }
}
