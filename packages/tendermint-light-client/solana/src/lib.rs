//! Solana-optimized Tendermint light client verifier using brine-ed25519
//!
//! TODO: For upstream PR to ibc-rs/tendermint-rs:
//! 1. tendermint-light-client-verifier/src/operations/voting_power.rs:319
//!    Add #[cfg(not(feature = "solana"))] before `votes.sort_unstable_by_key()`
//!    to skip on-chain sorting when signatures are pre-sorted by relayer
//! 2. solana-ibc-types/src/borsh_header.rs `conversions::commit_to_borsh()`
//!    Pre-sort signatures before serialization (saves ~60-80k CU on-chain)

use tendermint::crypto::signature::Error;
use tendermint::{crypto::signature, PublicKey, Signature};
use tendermint_light_client_verifier::{
    errors::VerificationError,
    operations::{commit_validator::ProdCommitValidator, ProvidedVotingPowerCalculator},
    predicates::VerificationPredicates,
    types::ValidatorSet,
    PredicateVerifier,
};

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
pub type SolanaVerifier =
    PredicateVerifier<SolanaPredicates, SolanaVotingPowerCalculator, ProdCommitValidator>;

/// Solana voting power calculator using optimized signature verification
pub type SolanaVotingPowerCalculator = ProvidedVotingPowerCalculator<SolanaSignatureVerifier>;

/// Solana optimised signature verifier
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SolanaSignatureVerifier;

impl signature::Verifier for SolanaSignatureVerifier {
    fn verify(pubkey: PublicKey, msg: &[u8], signature: &Signature) -> Result<(), Error> {
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
    fn empty_hash(&mut self) -> tendermint::merkle::Hash {
        self.0.empty_hash()
    }

    fn leaf_hash(&mut self, bytes: &[u8]) -> tendermint::merkle::Hash {
        self.0.leaf_hash(bytes)
    }

    fn inner_hash(
        &mut self,
        left: tendermint::merkle::Hash,
        right: tendermint::merkle::Hash,
    ) -> tendermint::merkle::Hash {
        self.0.inner_hash(left, right)
    }
}
