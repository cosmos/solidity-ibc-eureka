//! Attestor light client verification logic

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::IbcAttestorClientError,
    header::Header, verify_attestation,
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
/// # Errors
/// Returns an error if:
/// - The client is frozen
/// - The header attestation verification fails. see [`verify_attestation::verify_attesation`]
/// - The header's timestamp does not match the consensus state
/// - The header's timestamp is not monotonically increasing
pub fn verify_header(
    existing_trusted_consensus: Option<&ConsensusState>,
    existing_prev_trusted_consensus: Option<&ConsensusState>,
    existing_next_trusted_consensus: Option<&ConsensusState>,
    client_state: &ClientState,
    header: &Header,
) -> Result<(), IbcAttestorClientError> {
    if client_state.is_frozen {
        return Err(IbcAttestorClientError::ClientFrozen);
    }

    verify_attestation::verify_attestation(
        client_state,
        &header.attestation_data,
        &header.signatures,
        &header.public_keys,
    )?;

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
    use crate::test_utils::{packet_encoded_bytes, PACKET_COMMITMENTS_ENCODED, PUBKEYS, SIGNERS, SIGS};
    use secp256k1::{Message, Secp256k1, SecretKey};
    use sha2::{Digest, Sha256};

    use super::*;

    #[test]
    fn fails_on_frozon() {
        let frozen = ClientState { attestors: SIGNERS.clone(), min_required_sigs: 5, latest_height: 100, is_frozen: true };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let header = Header { new_height: cns.height, timestamp: cns.timestamp, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: SIGS.clone(), public_keys: PUBKEYS.clone() };

        let res = verify_header(Some(&cns), None, None, &frozen, &header);
        assert!(matches!(res, Err(IbcAttestorClientError::ClientFrozen)));
    }
    #[test]
    fn fails_on_too_few_sigs() {
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, is_frozen: false, min_required_sigs: 5 };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };

        let mut too_few_sigs = SIGS.to_vec();
        let _ = too_few_sigs.pop();
        let no_sig = Header { new_height: cns.height, timestamp: cns.timestamp, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: too_few_sigs, public_keys: PUBKEYS.clone() };

        let res = verify_header(Some(&cns), None, None, &cs, &no_sig);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidAttestedData { reason}) if reason.contains("signature"))
        );
    }

    #[test]
    fn fails_on_too_pubkeys() {
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, is_frozen: false, min_required_sigs: 5 };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };

        let mut too_few_pubkeys = PUBKEYS.to_vec();
        let _ = too_few_pubkeys.pop();
        let no_sig = Header { new_height: cns.height, timestamp: cns.timestamp, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: SIGS.to_vec(), public_keys: too_few_pubkeys };

        let res = verify_header(Some(&cns), None, None, &cs, &no_sig);
        assert!(matches!(res, Err(IbcAttestorClientError::InvalidAttestedData { .. })));
    }

    #[test]
    fn fails_on_rogue_signature() {
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, is_frozen: false, min_required_sigs: 5 };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };

        let secp = Secp256k1::new();
        let rogue_skey = SecretKey::from_slice(&[0x04; 32]).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&*packet_encoded_bytes());
        let digest = hasher.finalize();
        let msg = Message::from_digest_slice(&digest).unwrap();
        let (_rec_id, compact) = secp.sign_ecdsa_recoverable(&msg, &rogue_skey).serialize_compact();
        let rogue_sig = compact.to_vec();

        let mut bad_sigs = SIGS.clone();
        bad_sigs[0] = rogue_sig;

        let no_sig = Header { new_height: cns.height, timestamp: cns.timestamp, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: bad_sigs, public_keys: PUBKEYS.clone() };

        let res = verify_header(Some(&cns), None, None, &cs, &no_sig);
        assert!(matches!(res, Err(IbcAttestorClientError::InvalidSignature)));
    }

    #[test]
    fn fails_on_rogue_key() {
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, is_frozen: false, min_required_sigs: 5 };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };

        let secp = Secp256k1::new();
        let rogue_key = SecretKey::from_slice(&[0x04; 32]).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&*packet_encoded_bytes());
        let digest = hasher.finalize();
        let msg = Message::from_digest_slice(&digest).unwrap();
        let (_rec_id, compact) = secp.sign_ecdsa_recoverable(&msg, &rogue_key).serialize_compact();
        let rogue_sig = compact.to_vec();
        let mut sigs_with_rogue = SIGS.clone();
        sigs_with_rogue[4] = rogue_sig;

        // compute rogue public key (compressed)
        let pk = secp256k1::PublicKey::from_secret_key(&secp, &rogue_key);
        let rogue_pubkey = pk.serialize().to_vec();
        let mut pubkeys = PUBKEYS.clone();
        pubkeys[4] = rogue_pubkey;

        let no_sig = Header { new_height: cns.height, timestamp: cns.timestamp, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: sigs_with_rogue, public_keys: pubkeys };

        let res = verify_header(Some(&cns), None, None, &cs, &no_sig);
        assert!(matches!(res, Err(IbcAttestorClientError::UnknownSigner { .. })));
    }

    #[test]
    fn fails_on_dup_sigs() {
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, is_frozen: false, min_required_sigs: 5 };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };

        let mut bad_sigs = SIGS.clone();
        bad_sigs[0] = bad_sigs[1].clone();

        let no_sig = Header { new_height: cns.height, timestamp: cns.timestamp, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: bad_sigs, public_keys: PUBKEYS.clone() };

        let res = verify_header(Some(&cns), None, None, &cs, &no_sig);
        assert!(matches!(res, Err(IbcAttestorClientError::InvalidSignature)) || matches!(res, Err(IbcAttestorClientError::InvalidAttestedData { .. })) );
    }

    #[test]
    fn fails_on_dup_keys() {
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, is_frozen: false, min_required_sigs: 5 };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };

        let mut bad_pubkeys = PUBKEYS.clone();
        bad_pubkeys[0] = bad_pubkeys[1].clone();

        let no_sig = Header { new_height: cns.height, timestamp: cns.timestamp, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: SIGS.clone(), public_keys: bad_pubkeys };

        let res = verify_header(Some(&cns), None, None, &cs, &no_sig);
        assert!(matches!(res, Err(IbcAttestorClientError::DuplicateSigner { .. })));
    }

    #[test]
    fn fails_on_inconsistent_ts() {
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, is_frozen: false, min_required_sigs: 5 };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let bad_ts = Header { new_height: cns.height, timestamp: cns.timestamp + 1, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: SIGS.clone(), public_keys: PUBKEYS.clone() };

        let res = verify_header(Some(&cns), None, None, &cs, &bad_ts);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidHeader {reason}) if reason.contains("consensus"))
        );
    }

    #[test]
    fn fails_non_monotonic_ts() {
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, is_frozen: false, min_required_sigs: 5 };

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

        let not_inbetween = Header { new_height: 100 + 1, timestamp: next.timestamp + 3, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: SIGS.clone(), public_keys: PUBKEYS.clone() };

        let res = verify_header(None, Some(&prev), Some(&next), &cs, &not_inbetween);
        assert!(matches!(res, Err(IbcAttestorClientError::InvalidHeader { .. })));

        let not_before = Header { new_height: 100 - 1, timestamp: next.timestamp + 3, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: SIGS.clone(), public_keys: PUBKEYS.clone() };

        let res = verify_header(None, None, Some(&next), &cs, &not_before);
        assert!(matches!(res, Err(IbcAttestorClientError::InvalidHeader { .. })));

        let not_after = Header { new_height: 100 + 3, timestamp: prev.timestamp - 1, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: SIGS.clone(), public_keys: PUBKEYS.clone() };

        let res = verify_header(None, Some(&prev), None, &cs, &not_after);
        assert!(matches!(res, Err(IbcAttestorClientError::InvalidHeader { .. })));
    }

    #[test]
    fn succeeds_on_monotonic_ts() {
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, is_frozen: false, min_required_sigs: 5 };

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

        let inbetween = Header { new_height: 100 + 1, timestamp: 123456789 + 1, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: SIGS.clone(), public_keys: PUBKEYS.clone() };

        let res = verify_header(None, Some(&prev), Some(&next), &cs, &inbetween);
        assert!(res.is_ok());

        let before = Header { new_height: 100 - 1, timestamp: 123456789 - 1, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: SIGS.clone(), public_keys: PUBKEYS.clone() };

        let res = verify_header(None, None, Some(&next), &cs, &before);
        assert!(res.is_ok());

        let after = Header { new_height: 100 + 3, timestamp: prev.timestamp + 3, attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), signatures: SIGS.clone(), public_keys: PUBKEYS.clone() };

        let res = verify_header(None, Some(&prev), None, &cs, &after);
        assert!(res.is_ok());
    }
}
