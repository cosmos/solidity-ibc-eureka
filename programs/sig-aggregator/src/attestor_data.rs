use anyhow::Result;
use crate::rpc::{AggregateResponse, Attestation, SigPubkeyPair};
use alloy_primitives::FixedBytes;
use std::collections::HashMap;

pub const STATE_BYTE_LENGTH: usize = 12;
type State = FixedBytes<STATE_BYTE_LENGTH>;

// https://docs.rs/secp256k1/latest/secp256k1/ecdsa/struct.Signature.html#method.serialize_compact
pub const SIGNATURE_BYTE_LENGTH: usize = 64;
type Signature = FixedBytes<SIGNATURE_BYTE_LENGTH>;

// Compressed public key length
// https://docs.rs/secp256k1/latest/secp256k1/struct.PublicKey.html#method.serialize
pub const PUBKEY_BYTE_LENGTH: usize = 33;
type Pubkey = FixedBytes<PUBKEY_BYTE_LENGTH>;

/// Maps attested_data -> list of attestations
///
/// Structure:
/// ```text
/// Attested_data: 0x1234... (12 bytes)
///     [Attestation_A, Attestation_B]
/// Attested_data: 0x9876...
///     [Attestation_C, Attestation_D]
/// ```
#[derive(Debug, Clone, Default)]
pub struct AttestatorData {
    state_attestations: HashMap<State, Vec<Attestation>>,
}

impl AttestatorData {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, attestation: Attestation) -> Result<()> {
        attestation.validate().map_err(|e| anyhow::anyhow!("Invalid attestation: {e}"))?;

        let attested_data = State::try_from(attestation.attested_data.as_slice())
            .map_err(|e| anyhow::anyhow!("Failed to convert attested_data to State: {e}"))?;

        if let Some(attestations) = self.state_attestations.get_mut(&attested_data) {
            attestations.push(attestation);
            return Ok(());
        }

        self.state_attestations
            .insert(attested_data, vec![attestation]);
        Ok(())
    }

    #[must_use]
    pub fn get_quorum(&self, quorum: usize) -> Option<AggregateResponse> {
        self.state_attestations
            .iter()
            .find(|(_, attestations)| attestations.len() >= quorum)
            .map(|(state, attestations)| AggregateResponse {
                height: attestations[0].height,
                state: state.to_vec(),
                sig_pubkey_pairs: attestations
                    .iter()
                    .map(|a| SigPubkeyPair {
                        sig: a.signature.clone(),
                        pubkey: a.public_key.clone(),
                    })
                    .collect(),
            })
    }
}

// TODO: move this to a separate library IBC-138
impl Attestation {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.public_key.len() != PUBKEY_BYTE_LENGTH {
            return Err(anyhow::anyhow!(
                "Invalid pubkey length: {}",
                self.public_key.len()
            ));
        }
        Pubkey::try_from(self.public_key.as_slice())
            .map_err(|e| anyhow::anyhow!("Invalid pubkey: {:#?}", self.public_key).context(e))?;

        if self.attested_data.len() != STATE_BYTE_LENGTH {
            return Err(anyhow::anyhow!(
                "Invalid attested_data length: {}",
                self.attested_data.len()
            ));
        }
        State::try_from(self.attested_data.as_slice())
            .map_err(|e| anyhow::anyhow!("Invalid attested_data: {:#?}", self.attested_data).context(e))?;

        if self.signature.len() != SIGNATURE_BYTE_LENGTH {
            return Err(anyhow::anyhow!(
                "Invalid signature length: {}",
                self.signature.len()
            ));
        }
        Signature::try_from(self.signature.as_slice())
            .map_err(|e| anyhow::anyhow!("Invalid signature: {:?}", self.signature).context(e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill_bytes<const N: usize>(b: u8) -> Vec<u8> {
        vec![b; N]
    }

    #[test]
    fn ignores_states_below_quorum() {
        // We have a height 100 but only 1 signature < quorum 2
        let mut attestator_data = AttestatorData::new();

        attestator_data.insert(Attestation {
            attested_data: fill_bytes::<STATE_BYTE_LENGTH>(1),
            public_key: fill_bytes::<PUBKEY_BYTE_LENGTH>(0x03),
            height: 100,
            signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(0x04),
        });

        let latest = attestator_data.get_quorum(2); // Quorum 2
        assert!(latest.is_none(), "Should not return a state below quorum");
    }

    #[test]
    fn state_meeting_quorum() {
        let mut attestator_data = AttestatorData::new();
        let state = fill_bytes::<STATE_BYTE_LENGTH>(0xAA);
        let height = 123;
        attestator_data.insert(Attestation {
            attested_data: state.clone(),
            public_key: fill_bytes::<PUBKEY_BYTE_LENGTH>(0x21),
            height,
            signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(0x11),
        });

        attestator_data.insert(Attestation {
            attested_data: state.clone(),
            public_key: fill_bytes::<PUBKEY_BYTE_LENGTH>(0x22),
            height,
            signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(0x11),
        });

        let latest = attestator_data.get_quorum(2); // Quorum 2
        assert!(latest.is_some(), "Should return a state meeting quorum");
        let latest = latest.unwrap();
        assert_eq!(latest.height, height);
        assert_eq!(latest.state, state);
        // Should have two SigPubkeyPair entries
        assert_eq!(latest.sig_pubkey_pairs.len(), 2);
        // Check that the pairs contain the pubkeys we inserted
        let pubs: Vec<_> = latest
            .sig_pubkey_pairs
            .into_iter()
            .map(|p| p.pubkey)
            .collect();
        assert!(pubs.contains(&fill_bytes::<PUBKEY_BYTE_LENGTH>(0x21)));
        assert!(pubs.contains(&fill_bytes::<PUBKEY_BYTE_LENGTH>(0x22)));
    }
}
