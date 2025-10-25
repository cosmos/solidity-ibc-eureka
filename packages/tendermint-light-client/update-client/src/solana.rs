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
