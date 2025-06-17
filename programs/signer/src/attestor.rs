use secp256k1::hashes::{sha256, Hash};
use secp256k1::SecretKey;
use secp256k1::{ecdsa::Signature, Message, PublicKey};

use crate::attestor_error::AttestorError;
use crate::l2_client::{Header, L2Client};

#[derive(Debug)]
pub struct AttestationData {
    pub chain_id: u64,
    pub height: u64,
    pub state_root: Vec<u8>,
    pub timestamp: u64,
}

impl AttestationData {
    fn to_bytes(&self) -> Vec<u8> {
        [
            &self.chain_id.to_ne_bytes(),
            &self.height.to_ne_bytes(),
            self.state_root.as_slice(),
            &self.timestamp.to_ne_bytes(),
        ]
        .concat()
    }

    #[cfg(test)]
    pub fn test_new(chain_id: u64) -> Self {
        Self {
            chain_id,
            height: 0,
            state_root: [0].into(),
            timestamp: 0,
        }
    }
}

#[derive(Debug)]
pub struct Attestation {
    pub data: AttestationData,
    pub signature: Signature,
    pub pubkey: PublicKey,
}

pub struct Attestor<C> {
    pub client: C,
}

impl<C> Attestor<C>
where
    C: L2Client,
{
    pub fn create_attestation(
        &self,
        height: u64,
        sk: &SecretKey,
        pk: &PublicKey,
    ) -> Result<Attestation, AttestorError> {
        let header = self.client.fetch_header(height)?;

        let data = AttestationData {
            chain_id: header.chain_id(),
            height,
            state_root: header.state_root(),
            timestamp: header.timestamp(),
        };

        let digest = sha256::Hash::hash(&data.to_bytes());
        let message = Message::from_digest(digest.to_byte_array());
        let sig = sk.sign_ecdsa(message);

        Ok(Attestation {
            data,
            signature: sig,
            pubkey: pk.clone(),
        })
    }
}

#[cfg(test)]
mod create_attestation {
    use secp256k1::{generate_keypair, rand};

    use super::*;

    struct MockHeader;

    impl Header for MockHeader {
        fn chain_id(&self) -> u64 {
            0
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
        fn fetch_header(&self, _height: u64) -> Result<impl Header, AttestorError> {
            Ok(MockHeader)
        }
    }

    #[test]
    fn does_not_change_with_sk() {
        let (sk, pk) = generate_keypair(&mut rand::rng());
        let att = Attestor { client: MockClient };

        let first_att = att.create_attestation(1, &sk, &pk).expect("must succeed");
        let second_att = att.create_attestation(1, &sk, &pk).expect("must succeed");
        assert_eq!(first_att.pubkey, second_att.pubkey);
        assert_eq!(first_att.signature, second_att.signature);
    }

    #[test]
    fn does_change_with_different_sk() {
        let (sk, pk) = generate_keypair(&mut rand::rng());
        let att = Attestor { client: MockClient };
        let first_att = att.create_attestation(1, &sk, &pk).expect("must succeed");

        let (sk, pk) = generate_keypair(&mut rand::rng());
        let second_att = att.create_attestation(1, &sk, &pk).expect("must succeed");
        assert_ne!(first_att.pubkey, second_att.pubkey);
        assert_ne!(first_att.signature, second_att.signature);
    }

    #[test]
    fn contains_correct_pubkey() {
        let (sk, pk) = generate_keypair(&mut rand::rng());
        let att = Attestor { client: MockClient };

        let attestation = att.create_attestation(1, &sk, &pk).expect("must succeed");
        assert_eq!(attestation.pubkey, pk);
    }
}
