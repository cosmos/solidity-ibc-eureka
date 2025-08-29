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
    use alloy_sol_types::SolValue;

    use crate::test_utils::{ADDRESSES, PACKET_COMMITMENTS_ENCODED, SIGS_RAW};

    use super::*;

    #[test]
    fn fails_on_frozon() {
        let addresses = ADDRESSES.clone();
        let frozen = ClientState {
            attestor_addresses: addresses,
            latest_height: 100,
            is_frozen: true,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123,
        };
        let header = Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: SIGS_RAW.clone(),
        };

        let res = verify_header(Some(&cns), None, None, &frozen, &header);
        assert!(matches!(res, Err(IbcAttestorClientError::ClientFrozen)));
    }

    #[test]
    fn fails_on_too_few_sigs() {
        let cs = ClientState {
            attestor_addresses: ADDRESSES.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123,
        };

        let mut too_few_sigs = SIGS_RAW.to_vec();
        let _ = too_few_sigs.pop();
        let no_sig = Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: too_few_sigs,
        };

        let res = verify_header(Some(&cns), None, None, &cs, &no_sig);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidAttestedData { reason}) if reason.contains("too few"))
        );
    }

    #[test]
    fn fails_on_rogue_signature() {
        let cs = ClientState {
            attestor_addresses: ADDRESSES.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123,
        };

        // Create a fake signature from an unknown key (65 bytes)
        let mut rogue_sig_raw = vec![0xff; 65];
        rogue_sig_raw[64] = 0; // v value

        let mut bad_sigs = SIGS_RAW.clone();
        bad_sigs[0] = rogue_sig_raw;

        let header = Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: bad_sigs,
        };

        let res = verify_header(Some(&cns), None, None, &cs, &header);
        // This should fail with either InvalidSignature or UnknownAddressRecovered
        assert!(matches!(
            res,
            Err(IbcAttestorClientError::InvalidSignature
                | IbcAttestorClientError::UnknownAddressRecovered { .. })
        ));
    }

    #[test]
    fn fails_on_dup_sigs() {
        let cs = ClientState {
            attestor_addresses: ADDRESSES.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123,
        };

        let mut bad_sigs = SIGS_RAW.clone();
        bad_sigs[0] = bad_sigs[1].clone();

        let header = Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: bad_sigs,
        };

        let res = verify_header(Some(&cns), None, None, &cs, &header);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidAttestedData { reason }) if reason.contains("duplicate"))
        );
    }

    #[test]
    fn fails_on_inconsistent_ts() {
        let cs = ClientState {
            attestor_addresses: ADDRESSES.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123,
        };
        let bad_ts = Header {
            new_height: cns.height,
            timestamp: cns.timestamp + 1,
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: SIGS_RAW.clone(),
        };

        let res = verify_header(Some(&cns), None, None, &cs, &bad_ts);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidHeader {reason}) if reason.contains("consensus"))
        );
    }

    #[test]
    fn fails_non_monotonic_ts() {
        let cs = ClientState {
            attestor_addresses: ADDRESSES.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };

        let (prev, next) = (
            ConsensusState {
                height: 100,
                timestamp: 123,
            },
            ConsensusState {
                height: 100 + 2,
                timestamp: 123 + 2,
            },
        );

        let not_inbetween = Header {
            new_height: 100 + 1,
            timestamp: next.timestamp + 3,
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: SIGS_RAW.clone(),
        };

        let res = verify_header(None, Some(&prev), Some(&next), &cs, &not_inbetween);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidHeader {reason}) if reason.contains("between"))
        );

        let not_before = Header {
            new_height: 100 - 1,
            timestamp: next.timestamp + 3,
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: SIGS_RAW.clone(),
        };

        let res = verify_header(None, None, Some(&next), &cs, &not_before);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidHeader {reason}) if reason.contains("before"))
        );

        let not_after = Header {
            new_height: 100 + 3,
            timestamp: prev.timestamp - 1,
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: SIGS_RAW.clone(),
        };

        let res = verify_header(None, Some(&prev), None, &cs, &not_after);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidHeader {reason}) if reason.contains("after"))
        );
    }

    #[test]
    fn succeeds_on_monotonic_ts() {
        let cs = ClientState {
            attestor_addresses: ADDRESSES.clone(),
            latest_height: 100,
            is_frozen: false,
            min_required_sigs: 5,
        };

        let (prev, next) = (
            ConsensusState {
                height: 100,
                timestamp: 123,
            },
            ConsensusState {
                height: 100 + 2,
                timestamp: 123 + 2,
            },
        );

        let inbetween = Header {
            new_height: 100 + 1,
            timestamp: 123 + 1,
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: SIGS_RAW.clone(),
        };

        let res = verify_header(None, Some(&prev), Some(&next), &cs, &inbetween);
        assert!(res.is_ok(),);

        let before = Header {
            new_height: 100 - 1,
            timestamp: 123 - 1,
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: SIGS_RAW.clone(),
        };

        let res = verify_header(None, None, Some(&next), &cs, &before);
        assert!(res.is_ok(),);

        let after = Header {
            new_height: 100 + 3,
            timestamp: prev.timestamp + 3,
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: SIGS_RAW.clone(),
        };

        let res = verify_header(None, Some(&prev), None, &cs, &after);
        assert!(res.is_ok(),);
    }
}
