use secp256k1::PublicKey;
use thiserror::Error;

use crate::multi_sig_attestation::MultiSigAttestation;

#[derive(Error, Debug)]
pub enum VerifyMultiSigError {
    #[error("Failed to verify signature for public key {0}")]
    InvaildSignature(PublicKey),
}
