//! Attestor client state for IBC light client

use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

/// Minimal attestor client state for IBC light client
/// Contains only the essential information needed for client management
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientState {
    /// Attestor Ethereum addresses (20-byte addresses recovered from signatures)
    pub attestor_addresses: Vec<Address>,
    /// Minimum required signatures
    pub min_required_sigs: u8,
    /// Latest height for tracking progression
    pub latest_height: u64,
    /// Whether the client is frozen due to misbehavior
    pub is_frozen: bool,
}

impl ClientState {
    /// Construct a new client state from a list of public keys.
    ///
    /// Note: public keys are not stored directly in the client state; callers should convert
    /// them to addresses for verification elsewhere. This helper initializes a minimal state.
    #[must_use]
    pub fn new_from_pubkeys(
        pub_keys: Vec<k256::ecdsa::VerifyingKey>,
        min_required_sigs: u8,
        latest_height: u64,
    ) -> Self {
        // Derive Ethereum addresses from the provided public keys
        let attestor_addresses = pub_keys
            .into_iter()
            .map(|pk| {
                use sha3::{Digest as Sha3Digest, Keccak256};
                let uncompressed = pk.to_encoded_point(false);
                let hash = Keccak256::digest(&uncompressed.as_bytes()[1..]);
                let mut addr_bytes = [0u8; 20];
                addr_bytes.copy_from_slice(&hash[12..]);
                Address::from(addr_bytes)
            })
            .collect();

        Self {
            attestor_addresses,
            min_required_sigs,
            latest_height,
            is_frozen: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ClientState;
    use alloy_primitives::Address;
    use k256::ecdsa::SigningKey;
    use sha3::{Digest as Sha3Digest, Keccak256};

    fn expected_eth_address_from_signing_key(skey: &SigningKey) -> Address {
        let vk = skey.verifying_key();
        let uncompressed = vk.to_encoded_point(false);
        let hash = Keccak256::digest(&uncompressed.as_bytes()[1..]);
        let mut addr_bytes = [0u8; 20];
        addr_bytes.copy_from_slice(&hash[12..]);
        Address::from(addr_bytes)
    }

    #[test]
    fn address_derivation_from_pubkey_matches_keccak_last20() {
        let skey = SigningKey::from_bytes(&[0xcd; 32].into()).expect("valid key");
        let expected = expected_eth_address_from_signing_key(&skey);

        let client_state = ClientState::new_from_pubkeys(vec![skey.verifying_key().clone()], 1, 1);
        assert_eq!(client_state.attestor_addresses.len(), 1);
        assert_eq!(client_state.attestor_addresses[0], expected);
    }

    #[test]
    fn client_state_populates_addresses_from_multiple_pubkeys() {
        let keys = [
            SigningKey::from_bytes(&[0xcd; 32].into()).expect("k1"),
            SigningKey::from_bytes(&[0x02; 32].into()).expect("k2"),
            SigningKey::from_bytes(&[0x1F; 32].into()).expect("k3"),
        ];

        let expected: Vec<Address> = keys.iter().map(expected_eth_address_from_signing_key).collect();

        let pubkeys = keys.iter().map(|k| k.verifying_key().clone()).collect();
        let client_state = ClientState::new_from_pubkeys(pubkeys, 2, 42);

        assert_eq!(client_state.attestor_addresses, expected);
        assert_eq!(client_state.min_required_sigs, 2);
        assert_eq!(client_state.latest_height, 42);
        assert!(!client_state.is_frozen);
    }
}
