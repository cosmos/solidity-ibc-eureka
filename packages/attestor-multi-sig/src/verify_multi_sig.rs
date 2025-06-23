use secp256k1::hashes::{sha256, Hash};
use secp256k1::{Error, Message, PublicKey};
use thiserror::Error;

use crate::multi_sig_attestation::MultiSigAttestation;

pub fn verify_multi_sig_attestation(msa: &MultiSigAttestation) -> Result<(), VerifyMultiSigError> {
    for (pubkey, sig) in msa.pubkeys.iter().zip(msa.signatures.iter()) {
        let digest = sha256::Hash::hash(&msa.attestation_data.to_bytes());
        let message = Message::from_digest(digest.to_byte_array());
        let _ = sig.verify(message, pubkey).map_err(|e| match e {
            Error::IncorrectSignature => VerifyMultiSigError::IncorrectSignature(*pubkey),
            _ => VerifyMultiSigError::Unexpected(e.to_string()),
        })?;
    }

    Ok(())
}

#[derive(Error, Debug)]
pub enum VerifyMultiSigError {
    #[error("Failed to verify signature for public key {0}")]
    IncorrectSignature(PublicKey),
    #[error("Failed to verify attestation due to unexpected error {0}")]
    Unexpected(String),
}

#[cfg(test)]
mod verification {
    use crate::{
        attestor::Attestor,
        attestor_error::AttestorError,
        l2_client::{Header, L2Client},
    };
    use secp256k1::{generate_keypair, rand};

    use super::*;

    struct MockHeader(u64);

    impl Header for MockHeader {
        fn chain_id(&self) -> u64 {
            self.0
        }
        fn state_root(&self) -> Vec<u8> {
            vec![0]
        }
        fn timestamp(&self) -> u64 {
            0
        }
    }

    struct MockClient;

    impl L2Client for MockClient {
        fn fetch_header(&self, chain_id: u64) -> Result<impl Header, AttestorError> {
            Ok(MockHeader(chain_id))
        }
    }

    #[test]
    fn fails_when_keys_are_mixed_up() {
        let (sk, pk) = generate_keypair(&mut rand::rng());
        let (mismatch_sk, mismatch_pk) = generate_keypair(&mut rand::rng());
        let att = Attestor { client: MockClient };

        let first = att.create_attestation(2, &sk, &pk).expect("must succeed");
        let second = att
            .create_attestation(2, &mismatch_sk, &mismatch_pk)
            .expect("must succeed");
        let third = att
            .create_attestation(2, &sk, &mismatch_pk)
            .expect("must succeed");

        let multi_sig =
            MultiSigAttestation::new([first, second, third].into()).expect("must succeed");

        let failed_verification = verify_multi_sig_attestation(&multi_sig);

        assert!(matches!(
            failed_verification,
            Err(VerifyMultiSigError::IncorrectSignature(bad_pk)) if bad_pk == mismatch_pk
        ));
    }

    #[test]
    fn fails_when_sigantures_are_mixed_up() {
        let (sk, pk) = generate_keypair(&mut rand::rng());
        let (mismatch_sk, mismatch_pk) = generate_keypair(&mut rand::rng());
        let att = Attestor { client: MockClient };

        let first = att.create_attestation(2, &sk, &pk).expect("must succeed");
        let second = att
            .create_attestation(2, &mismatch_sk, &mismatch_pk)
            .expect("must succeed");
        let third = att
            .create_attestation(2, &mismatch_sk, &pk)
            .expect("must succeed");

        let multi_sig =
            MultiSigAttestation::new([first, second, third].into()).expect("must succeed");

        let failed_verification = verify_multi_sig_attestation(&multi_sig);

        assert!(matches!(
            failed_verification,
            Err(VerifyMultiSigError::IncorrectSignature(bad_pk)) if bad_pk == pk
        ));
    }

    #[test]
    fn succeeds_when_msa_correct() {
        let (sk, pk) = generate_keypair(&mut rand::rng());
        let (another_sk, another_pk) = generate_keypair(&mut rand::rng());
        let att = Attestor { client: MockClient };

        let first = att.create_attestation(2, &sk, &pk).expect("must succeed");
        let second = att
            .create_attestation(2, &another_sk, &another_pk)
            .expect("must succeed");

        let multi_sig = MultiSigAttestation::new([first, second].into()).expect("must succeed");

        let verified = verify_multi_sig_attestation(&multi_sig);

        assert!(verified.is_ok());
    }
}
