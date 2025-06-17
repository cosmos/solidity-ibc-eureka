use crate::attestor::{Attestation, AttestationData};
use secp256k1::ecdsa::Signature;
use secp256k1::PublicKey;
use thiserror::Error;

/// A multi-signature attestation, collecting N individual attestations on the same data.
/// Ensures all attestations refer to identical state and preserves public keys and signatures in order.
#[derive(Debug)]
pub struct MultiSigAttestation {
    pub chain_id: u64,
    pub height: u64,
    pub state_root: Vec<u8>,
    pub timestamp: u64,
    pub pubkeys: Vec<PublicKey>,
    pub signatures: Vec<Signature>,
}

impl MultiSigAttestation {
    /// Constructs a MultiSigAttestation from a non-empty list of Attestation.
    /// Returns Err if the list is empty or if any attestation's data differs from the first.
    pub fn new(attestations: Vec<Attestation>) -> Result<Self, MultiSigAttestorError> {
        if attestations.is_empty() {
            return Err(MultiSigAttestorError::NoAttestations);
        }

        let first = &attestations[0];
        let chain_id = first.data.chain_id;
        let height = first.data.height;
        let state_root = first.data.state_root.clone();
        let timestamp = first.data.timestamp;

        let mut pubkeys = Vec::with_capacity(attestations.len());
        let mut signatures = Vec::with_capacity(attestations.len());

        for att in attestations.iter() {
            if att.data.chain_id != chain_id
                || att.data.height != height
                || att.data.state_root != state_root
                || att.data.timestamp != timestamp
            {
                let keys_to_data = attestations
                    .into_iter()
                    .map(|d| (d.pubkey, d.data))
                    .collect();
                return Err(MultiSigAttestorError::InconsistentData(keys_to_data));
            }
            pubkeys.push(att.pubkey.clone());
            signatures.push(att.signature.clone());
        }

        Ok(MultiSigAttestation {
            chain_id,
            height,
            state_root,
            timestamp,
            pubkeys,
            signatures,
        })
    }
}

#[derive(Error, Debug)]
pub enum MultiSigAttestorError {
    #[error("No attestations provided for multi sig")]
    NoAttestations,
    #[error("Data signed by attestors differs: {0:?}")]
    InconsistentData(Vec<(PublicKey, AttestationData)>),
}

#[cfg(test)]
mod constructor {
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
    fn fails_on_empty() {
        assert!(matches!(
            MultiSigAttestation::new(Vec::new()),
            Err(MultiSigAttestorError::NoAttestations)
        ));
    }

    #[test]
    fn fails_when_first_is_bad() {
        let (sk, pk) = generate_keypair(&mut rand::rng());
        let att = Attestor { client: MockClient };

        let first = att.create_attestation(777, &sk, &pk).expect("must succeed");
        let second = att.create_attestation(2, &sk, &pk).expect("must succeed");
        let third = att.create_attestation(2, &sk, &pk).expect("must succeed");

        assert!(matches!(
            MultiSigAttestation::new([first, second, third].into()),
            Err(MultiSigAttestorError::InconsistentData(_))
        ));
    }

    #[test]
    fn fails_on_last_bad() {
        let (sk, pk) = generate_keypair(&mut rand::rng());
        let att = Attestor { client: MockClient };

        let first = att.create_attestation(2, &sk, &pk).expect("must succeed");
        let second = att.create_attestation(2, &sk, &pk).expect("must succeed");
        let third = att.create_attestation(777, &sk, &pk).expect("must succeed");

        assert!(matches!(
            MultiSigAttestation::new([first, second, third].into()),
            Err(MultiSigAttestorError::InconsistentData(_))
        ));
    }

    #[test]
    fn succeeds_with_correct_pubkey_and_sigs() {
        let (sk, pk) = generate_keypair(&mut rand::rng());
        let att = Attestor { client: MockClient };

        let first = att.create_attestation(2, &sk, &pk).expect("must succeed");
        let second = att.create_attestation(2, &sk, &pk).expect("must succeed");
        let third = att.create_attestation(2, &sk, &pk).expect("must succeed");

        let result = MultiSigAttestation::new([first, second, third].into());
        assert!(result.is_ok());

        let multi_sig = result.unwrap();
        assert_eq!(multi_sig.signatures.len(), 3);
        assert_eq!(multi_sig.pubkeys.len(), 3);
    }
}
