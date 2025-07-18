//! Attestor light client verification logic

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::SolanaIBCError,
    header::Header,
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
/// - The header has no signature data
/// - The header's timestamp does not match the consensus state
/// - The header's timestamp is not monotonically increasing
pub fn verify_header(
    existing_trusted_consensus: Option<&ConsensusState>,
    existing_prev_trusted_consensus: Option<&ConsensusState>,
    existing_next_trusted_consensus: Option<&ConsensusState>,
    client_state: &ClientState,
    header: &Header,
) -> Result<(), SolanaIBCError> {
    if client_state.is_frozen {
        return Err(SolanaIBCError::ClientFrozen);
    }

    if header.signature_data.is_empty() {
        return Err(SolanaIBCError::InvalidHeader {
            reason: "signature must contain data".into(),
        });
    }

    if let Some(trusted_consensus) = existing_trusted_consensus {
        if header.timestamp != trusted_consensus.timestamp {
            return Err(SolanaIBCError::InvalidHeader {
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
                return Err(SolanaIBCError::InvalidHeader {
                    reason:
                        "timestamp must increase monotonically between previous and next timestamps"
                            .into(),
                });
            }
        }
        (Some(prev), None) => {
            if header.timestamp < prev.timestamp {
                return Err(SolanaIBCError::InvalidHeader {
                    reason: "timestamp must increase monotonically after previous timestamp".into(),
                });
            }
        }

        (None, Some(next)) => {
            if header.timestamp > next.timestamp {
                return Err(SolanaIBCError::InvalidHeader {
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
    use ibc_proto_eureka::cosmos::crypto::secp256k1::PubKey;
    use std::cell::LazyCell;
    pub const KEYS: LazyCell<[PubKey; 5]> = LazyCell::new(|| {
        [
            PubKey::default(),
            PubKey::default(),
            PubKey::default(),
            PubKey::default(),
            PubKey::default(),
        ]
    });

    use super::*;

    #[test]
    fn fails_on_frozon() {
        let frozen = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: true,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let header = Header {
            new_height: cns.height,
            timestamp: cns.timestamp,
            signature_data: [0].into(),
        };

        let res = verify_header(Some(&cns), None, None, &frozen, &header);
        assert!(matches!(res, Err(SolanaIBCError::ClientFrozen)));
    }

    #[test]
    fn fails_on_empty_signature() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let no_sig = Header {
            new_height: cns.height,
            timestamp: cns.timestamp + 1,
            signature_data: [].into(),
        };

        let res = verify_header(Some(&cns), None, None, &cs, &no_sig);
        assert!(
            matches!(res, Err(SolanaIBCError::InvalidHeader {reason}) if reason.contains("signature"))
        );
    }

    #[test]
    fn fails_on_inconsistent_ts() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
        };
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let bad_ts = Header {
            new_height: cns.height,
            timestamp: cns.timestamp + 1,
            signature_data: [0].into(),
        };

        let res = verify_header(Some(&cns), None, None, &cs, &bad_ts);
        assert!(
            matches!(res, Err(SolanaIBCError::InvalidHeader {reason}) if reason.contains("consensus"))
        );
    }

    #[test]
    fn fails_non_monotonic_ts() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
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
            signature_data: [0].into(),
        };

        let res = verify_header(None, Some(&prev), Some(&next), &cs, &not_inbetween);
        assert!(
            matches!(res, Err(SolanaIBCError::InvalidHeader {reason}) if reason.contains("between"))
        );

        let not_before = Header {
            new_height: 100 - 1,
            timestamp: next.timestamp + 3,
            signature_data: [0].into(),
        };

        let res = verify_header(None, None, Some(&next), &cs, &not_before);
        assert!(
            matches!(res, Err(SolanaIBCError::InvalidHeader {reason}) if reason.contains("before"))
        );

        let not_after = Header {
            new_height: 100 + 3,
            timestamp: prev.timestamp - 1,
            signature_data: [0].into(),
        };

        let res = verify_header(None, Some(&prev), None, &cs, &not_after);
        assert!(
            matches!(res, Err(SolanaIBCError::InvalidHeader {reason}) if reason.contains("after"))
        );
    }

    #[test]
    fn succeeds_on_monotonic_ts() {
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            is_frozen: false,
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
            signature_data: [0].into(),
        };

        let res = verify_header(None, Some(&prev), Some(&next), &cs, &inbetween);
        assert!(res.is_ok(),);

        let before = Header {
            new_height: 100 - 1,
            timestamp: 123456789 - 1,
            signature_data: [0].into(),
        };

        let res = verify_header(None, None, Some(&next), &cs, &before);
        assert!(res.is_ok(),);

        let after = Header {
            new_height: 100 + 3,
            timestamp: prev.timestamp + 3,
            signature_data: [0].into(),
        };

        let res = verify_header(None, Some(&prev), None, &cs, &after);
        assert!(res.is_ok(),);
    }
}
