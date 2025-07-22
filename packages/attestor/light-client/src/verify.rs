//! Attestor light client verification logic

use crate::{
    client_state::ClientState,
    consensus_state::ConsensusState,
    error::IbcAttestorClientError,
    header::Header,
    verify_attestation::{self, Verifyable},
};

/// Verifies the header of the light client
///
/// Trusted consensus state must be retvieved using the header
/// height.
///
/// Assumes that optional previous and next consensuses have
/// been reliably retrieved using height. Only a timestamp
/// validation takes place
///
/// Returns an error if:
/// - The client is frozen
/// - The header cannot marshalled to [Verifyable]
/// - The header attestation verification fails. see [verify_attestation::verify_attesation]
/// - The header's timestamp does not match the consensus state
/// - The header's timestamp is not monotonically increasing
pub fn verify_header(
    existing_trusted_consensus: Option<&ConsensusState>,
    existing_prev_trusted_consensus: Option<&ConsensusState>,
    existing_next_trusted_consensus: Option<&ConsensusState>,
    client_state: &ClientState,
    header: Header,
) -> Result<(), IbcAttestorClientError> {
    if client_state.is_frozen {
        return Err(IbcAttestorClientError::ClientFrozen);
    }
    let verifiable = Verifyable::new(
        header.attestation_data,
        header.signatures,
        header.pubkeys,
        client_state,
    )?;

    let _ = verify_attestation::verify_attesation(client_state, &verifiable)?;

    if let Some(trusted_consensus) = existing_trusted_consensus {
        if header.timestamp != trusted_consensus.timestamp {
            return Err(IbcAttestorClientError::InvalidHeader {
                reason: "timestamp does not match consensus state".into(),
            });
        }
        return Ok(());
    }

    match (
        existing_prev_trusted_consensus,
        existing_next_trusted_consensus,
    ) {
        (Some(prev), Some(next)) => {
            if !(header.timestamp > prev.timestamp && header.timestamp < next.timestamp) {
                return Err(IbcAttestorClientError::InvalidHeader {
                    reason:
                        "timestamp must increase monotonically between previous and next timestamps"
                            .into(),
                });
            }
        }
        (Some(prev), None) => {
            if header.timestamp < prev.timestamp {
                return Err(IbcAttestorClientError::InvalidHeader {
                    reason: "timestamp must increase monotonically after previous timestamp".into(),
                });
            }
        }

        (None, Some(next)) => {
            if header.timestamp > next.timestamp {
                return Err(IbcAttestorClientError::InvalidHeader {
                    reason: "timestamp must increase monotonically before next timestamp".into(),
                });
            }
        }
        // First in storage
        (None, None) => {}
    }

    Ok(())
}

#[cfg(test)]
mod verify_header {
    use secp256k1::{ecdsa::Signature, hashes::Hash, Message, PublicKey, SecretKey};
    use std::cell::LazyCell;

    use super::*;

    pub const DUMMY_DATA: [u8; 1] = [0];
    pub const S_KEYS: LazyCell<[SecretKey; 5]> = LazyCell::new(|| {
        [
            SecretKey::from_byte_array([0xcd; 32]).expect("32 bytes, within curve order"),
            SecretKey::from_byte_array([0x02; 32]).expect("32 bytes, within curve order"),
            SecretKey::from_byte_array([0x03; 32]).expect("32 bytes, within curve order"),
            SecretKey::from_byte_array([0x10; 32]).expect("32 bytes, within curve order"),
            SecretKey::from_byte_array([0x1F; 32]).expect("32 bytes, within curve order"),
        ]
    });
    pub const KEYS: LazyCell<[PublicKey; 5]> = LazyCell::new(|| {
        [
            PublicKey::from_secret_key_global(&S_KEYS[0]),
            PublicKey::from_secret_key_global(&S_KEYS[1]),
            PublicKey::from_secret_key_global(&S_KEYS[2]),
            PublicKey::from_secret_key_global(&S_KEYS[3]),
            PublicKey::from_secret_key_global(&S_KEYS[4]),
        ]
    });
    pub const SIGS: LazyCell<Vec<Signature>> = LazyCell::new(|| {
        let sigs = S_KEYS
            .iter()
            .map(|skey| {
                let digest = secp256k1::hashes::sha256::Hash::hash(&DUMMY_DATA);
                let message = Message::from_digest(digest.to_byte_array());
                skey.sign_ecdsa(message)
            })
            .collect();

        sigs
    });

    #[test]
    fn fails_on_frozon() {
        let frozen = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: true,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let header = Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: SIGS.clone(),
            pubkeys: KEYS.clone().into(),
        };

        let res = verify_header(Some(&cns), None, None, &frozen, header);
        assert!(matches!(res, Err(IbcAttestorClientError::ClientFrozen)));
    }
    #[test]
    fn fails_on_too_few_sigs() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };

        let mut too_few_sigs = SIGS.to_vec();
        let _ = too_few_sigs.pop();
        let no_sig = Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: too_few_sigs,
            pubkeys: KEYS.clone().into(),
        };

        let res = verify_header(Some(&cns), None, None, &cs, no_sig);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidAttestedData { reason}) if reason.contains("signature"))
        );
    }

    #[test]
    fn fails_on_too_pubkeys() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };

        let mut too_few_keys = KEYS.to_vec();
        let _ = too_few_keys.pop();
        let no_sig = Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: SIGS.to_vec(),
            pubkeys: too_few_keys,
        };

        let res = verify_header(Some(&cns), None, None, &cs, no_sig);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidAttestedData { reason}) if reason.contains("keys"))
        );
    }

    #[test]
    fn fails_on_rogue_signature() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };

        let rogue_sig =
            SecretKey::from_byte_array([0x04; 32]).expect("32 bytes, within curve order");

        let digest = secp256k1::hashes::sha256::Hash::hash(&DUMMY_DATA);
        let message = Message::from_digest(digest.to_byte_array());
        let rogue_sig = rogue_sig.sign_ecdsa(message);

        let mut bad_sigs = SIGS.clone();
        bad_sigs[0] = rogue_sig;

        let no_sig = Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: bad_sigs,
            pubkeys: KEYS.clone().into(),
        };

        let res = verify_header(Some(&cns), None, None, &cs, no_sig);
        assert!(matches!(res, Err(IbcAttestorClientError::InvalidSignature)));
    }

    #[test]
    fn fails_on_rogue_key() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };

        let rogue_key =
            SecretKey::from_byte_array([0x04; 32]).expect("32 bytes, within curve order");
        let digest = secp256k1::hashes::sha256::Hash::hash(&DUMMY_DATA);
        let message = Message::from_digest(digest.to_byte_array());
        let rogue_sig = rogue_key.sign_ecdsa(message);
        let mut valid_sigs_with_rogue_signer = SIGS.clone();
        valid_sigs_with_rogue_signer[4] = rogue_sig;

        let rogue_pkey = PublicKey::from_secret_key_global(&rogue_key);
        let mut rogue_keys = KEYS.clone();
        rogue_keys[4] = rogue_pkey;

        let no_sig = Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: valid_sigs_with_rogue_signer,
            pubkeys: rogue_keys.to_vec(),
        };

        let res = verify_header(Some(&cns), None, None, &cs, no_sig);
        assert!(matches!(
            res,
            Err(IbcAttestorClientError::UnknownPublicKeySubmitted { pubkey } ) if pubkey == rogue_pkey
        ));
    }

    #[test]
    fn fails_on_dup_sigs() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };

        let mut bad_sigs = SIGS.clone();
        bad_sigs[0] = bad_sigs[1].clone();

        let no_sig = Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: bad_sigs,
            pubkeys: KEYS.clone().into(),
        };

        let res = verify_header(Some(&cns), None, None, &cs, no_sig);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidAttestedData { reason }) if reason.contains("signature"))
        );
    }

    #[test]
    fn fails_on_dup_keys() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };

        let mut bad_keys = KEYS.clone();
        bad_keys[0] = bad_keys[1].clone();

        let no_sig = Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: SIGS.clone(),
            pubkeys: bad_keys.to_vec(),
        };

        let res = verify_header(Some(&cns), None, None, &cs, no_sig);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidAttestedData { reason }) if reason.contains("keys"))
        );
    }

    #[test]
    fn fails_on_inconsistent_ts() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let bad_ts = Header {
            new_height: cns.height,
            timestamp: cns.timestamp + 1,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: SIGS.clone(),
            pubkeys: KEYS.clone().into(),
        };

        let res = verify_header(Some(&cns), None, None, &cs, bad_ts);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidHeader {reason}) if reason.contains("consensus"))
        );
    }

    #[test]
    fn fails_non_monotonic_ts() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };

        let (prev, next) = (
            ConsensusState {
                height: 100,
                timestamp: 123456789,
            },
            ConsensusState {
                height: 100 + 2,
                timestamp: 123456789 + 2,
            },
        );

        let not_inbetween = Header {
            new_height: 100 + 1,
            timestamp: next.timestamp + 3,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: SIGS.clone(),
            pubkeys: KEYS.clone().into(),
        };

        let res = verify_header(None, Some(&prev), Some(&next), &cs, not_inbetween);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidHeader {reason}) if reason.contains("between"))
        );

        let not_before = Header {
            new_height: 100 - 1,
            timestamp: next.timestamp + 3,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: SIGS.clone(),
            pubkeys: KEYS.clone().into(),
        };

        let res = verify_header(None, None, Some(&next), &cs, not_before);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidHeader {reason}) if reason.contains("before"))
        );

        let not_after = Header {
            new_height: 100 + 3,
            timestamp: prev.timestamp - 1,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: SIGS.clone(),
            pubkeys: KEYS.clone().into(),
        };

        let res = verify_header(None, Some(&prev), None, &cs, not_after);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidHeader {reason}) if reason.contains("after"))
        );
    }

    #[test]
    fn succeeds_on_monotonic_ts() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };

        let (prev, next) = (
            ConsensusState {
                height: 100,
                timestamp: 123456789,
            },
            ConsensusState {
                height: 100 + 2,
                timestamp: 123456789 + 2,
            },
        );

        let inbetween = Header {
            new_height: 100 + 1,
            timestamp: 123456789 + 1,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: SIGS.clone(),
            pubkeys: KEYS.clone().into(),
        };

        let res = verify_header(None, Some(&prev), Some(&next), &cs, inbetween);
        assert!(res.is_ok(),);

        let before = Header {
            new_height: 100 - 1,
            timestamp: 123456789 - 1,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: SIGS.clone(),
            pubkeys: KEYS.clone().into(),
        };

        let res = verify_header(None, None, Some(&next), &cs, before);
        assert!(res.is_ok(),);

        let after = Header {
            new_height: 100 + 3,
            timestamp: prev.timestamp + 3,
            attestation_data: DUMMY_DATA.clone().into(),
            signatures: SIGS.clone(),
            pubkeys: KEYS.clone().into(),
        };

        let res = verify_header(None, Some(&prev), None, &cs, after);
        assert!(res.is_ok(),);
    }
}
