use crate::rpc::{Attestation, GetStateAttestationResponse, SigPubkeyPair};
use alloy_primitives::FixedBytes;
use anyhow::{ensure as anyhow_ensure, Context, Result};
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
        attestation.validate().context("Invalid attestation")?;

        let attested_data = State::try_from(attestation.attested_data.as_slice())
            .context("Failed to convert attested_data to State")?;

        self.state_attestations
            .entry(attested_data)
            .or_default()
            .push(attestation);

        Ok(())
    }

    #[must_use]
    pub fn agg_quorumed_attestations(&self, quorum: usize) -> Option<GetStateAttestationResponse> {
        self.state_attestations
            .iter()
            .find(|(_, attestations)| attestations.len() >= quorum)
            .map(|(state, attestations)| {
                let sig_pubkey_pairs = attestations
                    .iter()
                    .map(|a| SigPubkeyPair {
                        sig: a.signature.clone(),
                        pubkey: a.public_key.clone(),
                    })
                    .collect();

                GetStateAttestationResponse {
                    height: attestations[0].height,
                    state: state.to_vec(),
                    sig_pubkey_pairs,
                }
            })
    }
}

// TODO: move this to a separate library IBC-138
impl Attestation {
    fn validate(&self) -> Result<()> {
        self.validate_pubkey()?;
        self.validate_attested_data()?;
        self.validate_signature()?;
        Ok(())
    }

    fn validate_pubkey(&self) -> Result<()> {
        anyhow_ensure!(
            self.public_key.len() == PUBKEY_BYTE_LENGTH,
            "Invalid pubkey length: {}",
            self.public_key.len()
        );

        Pubkey::try_from(self.public_key.as_slice())
            .with_context(|| format!("Invalid pubkey: {:?}", self.public_key))?;

        Ok(())
    }

    fn validate_attested_data(&self) -> Result<()> {
        anyhow_ensure!(
            self.attested_data.len() == STATE_BYTE_LENGTH,
            "Invalid attested_data length: {}",
            self.attested_data.len()
        );

        State::try_from(self.attested_data.as_slice())
            .with_context(|| format!("Invalid attested_data: {:#?}", self.attested_data))?;

        Ok(())
    }

    fn validate_signature(&self) -> Result<()> {
        anyhow_ensure!(
            self.signature.len() == SIGNATURE_BYTE_LENGTH,
            "Invalid signature length: {}",
            self.signature.len()
        );

        Signature::try_from(self.signature.as_slice())
            .with_context(|| format!("Invalid signature: {:?}", self.signature))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_states_below_quorum() {
        // We have a height 100 but only 1 signature < quorum 2
        let mut attestator_data = AttestatorData::new();

        attestator_data
            .insert(Attestation {
                attested_data: vec![1; STATE_BYTE_LENGTH],
                public_key: vec![0x03; PUBKEY_BYTE_LENGTH],
                height: 100,
                signature: vec![0x04; SIGNATURE_BYTE_LENGTH],
            })
            .unwrap();

        let latest = attestator_data.agg_quorumed_attestations(2);
        assert!(latest.is_none(), "Should not return a state below quorum");
    }

    #[test]
    fn state_meeting_quorum() {
        let mut attestator_data = AttestatorData::new();
        let state = vec![0xAA; STATE_BYTE_LENGTH];
        let height = 123;

        [0x21, 0x22].iter().for_each(|&pubkey_byte| {
            attestator_data
                .insert(Attestation {
                    attested_data: state.clone(),
                    public_key: vec![pubkey_byte; PUBKEY_BYTE_LENGTH],
                    height,
                    signature: vec![0x11; SIGNATURE_BYTE_LENGTH],
                })
                .unwrap();
        });

        let latest = attestator_data.agg_quorumed_attestations(2);
        assert!(latest.is_some(), "Should return a state meeting quorum");

        let latest = latest.unwrap();
        assert_eq!(latest.height, height);
        assert_eq!(latest.state, state);
        // Should have two SigPubkeyPair entries
        assert_eq!(latest.sig_pubkey_pairs.len(), 2);

        let pubkeys: Vec<_> = latest
            .sig_pubkey_pairs
            .into_iter()
            .map(|p| p.pubkey)
            .collect();

        assert!(pubkeys.contains(&vec![0x21; PUBKEY_BYTE_LENGTH]));
        assert!(pubkeys.contains(&vec![0x22; PUBKEY_BYTE_LENGTH]));
    }
}
