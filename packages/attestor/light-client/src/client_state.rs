//! Attestor client state for IBC light client

use secp256k1::PublicKey;
use serde::{Deserialize, Serialize};

use crate::error::IbcAttestorClientError;

/// Minimal attestor client state for IBC light client
/// Contains only the essential information needed for client management
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientState {
    /// Attestor public keys
    pub pub_keys: Vec<PublicKey>,
    /// Minimum required signatures
    pub min_required_sigs: u8,
    /// Latest height for tracking progression
    pub latest_height: u64,
    /// Whether the client is frozen due to misbehavior
    pub is_frozen: bool,
}

impl ClientState {
    /// Replaces the public keys for a client using
    /// compressed by representations of public keys
    #[must_use]

    pub fn replace_pub_keys<K: AsRef<[u8]>>(
        &mut self,
        keys: &[K],
    ) -> Result<(), IbcAttestorClientError> {
        let serialized = keys
            .iter()
            .map(|k| PublicKey::from_slice(k.as_ref()))
            .collect::<Result<Vec<PublicKey>, _>>()
            .map_err(|_| IbcAttestorClientError::MalformedPublicKeySubmitted)?;

        self.pub_keys = serialized;
        Ok(())
    }
}
