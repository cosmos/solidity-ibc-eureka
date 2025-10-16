//! Solana-optimized Tendermint light client verifier using brine-ed25519

use tendermint::crypto::signature::Error;
use tendermint::{crypto::signature, PublicKey, Signature};
use tendermint_light_client_verifier::{
    operations::{commit_validator::ProdCommitValidator, ProvidedVotingPowerCalculator},
    predicates::ProdPredicates,
    PredicateVerifier,
};

/// Solana-optimized verifier that uses brine-ed25519 for signature verification
pub type SolanaVerifier =
    PredicateVerifier<ProdPredicates, SolanaVotingPowerCalculator, ProdCommitValidator>;

/// Solana voting power calculator using optimized signature verification
pub type SolanaVotingPowerCalculator = ProvidedVotingPowerCalculator<SolanaSignatureVerifier>;

/// Solana optimised signature verifier
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SolanaSignatureVerifier;

impl signature::Verifier for SolanaSignatureVerifier {
    fn verify(pubkey: PublicKey, msg: &[u8], signature: &Signature) -> Result<(), Error> {
        match pubkey {
            // NOTE: Custom ed25519 for Solana constraints - uses native curve ops, enables early exit
            // ~30k CU cost vs Ed25519Program (unavailable when verifying signatures from external data
            // that wasn't included as Ed25519Program instructions in current transaction i.e.
            // header chunks)
            // Alternative: Multi-transaction batching with Ed25519Program but adds complexity,
            // state management overhead, and atomicity concerns
            PublicKey::Ed25519(pk) => {
                brine_ed25519::sig_verify(pk.as_bytes(), signature.as_bytes(), msg)
                    .map_err(|_| Error::VerificationFailed)
            }
            _ => Err(Error::UnsupportedKeyType),
        }
    }
}
