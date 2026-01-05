use anchor_lang::prelude::*;
use anchor_lang::Space;

/// Ethereum address new-type (20 bytes)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub struct EthereumAddress(pub [u8; 20]);

impl Space for EthereumAddress {
    const INIT_SPACE: usize = 20;
}

impl EthereumAddress {
    /// Create a new Ethereum address from a byte array
    pub const fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Get the underlying byte array
    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

impl From<[u8; 20]> for EthereumAddress {
    fn from(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8; 20]> for EthereumAddress {
    fn as_ref(&self) -> &[u8; 20] {
        &self.0
    }
}

/// Result of an update_client operation
/// Matches ILightClientMsgs.UpdateResult from Solidity (contracts/msgs/ILightClientMsgs.sol:32-39)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum UpdateResult {
    /// The update was successful
    Update,
    /// A misbehavior was detected
    Misbehavior,
    /// Client is already up to date
    NoOp,
}

/// Client state for the attestation light client. Stores the fixed attestor
/// set, quorum threshold, and latest verified height
#[account]
#[derive(InitSpace)]
pub struct ClientState {
    /// Fixed list of attestor Ethereum addresses (20 bytes each). Maximum of 10 attestors supported
    #[max_len(10)]
    pub attestor_addresses: Vec<EthereumAddress>,

    /// Minimum number of signatures required (m-of-n quorum)
    pub min_required_sigs: u8,

    /// Latest known height that has been verified
    pub latest_height: u64,

    /// Whether the client is frozen due to misbehavior
    pub is_frozen: bool,
}

impl ClientState {
    pub const SEED: &'static [u8] = b"attestation_client";

    /// Check if the client is frozen
    pub const fn is_frozen(&self) -> bool {
        self.is_frozen
    }

    /// Freeze the client due to misbehavior
    pub fn freeze(&mut self) {
        self.is_frozen = true;
    }

    /// Check if an Ethereum address is in the attestor set
    pub fn is_attestor(&self, address: &EthereumAddress) -> bool {
        self.attestor_addresses.iter().any(|addr| addr == address)
    }
}

/// Consensus state storage for a specific height. Stores the consensus
/// timestamp at a particular height
#[account]
#[derive(InitSpace)]
pub struct ConsensusStateStore {
    /// The height of this consensus state
    pub height: u64,

    /// The consensus timestamp at this height (Unix seconds)
    pub timestamp: u64,
}

impl ConsensusStateStore {
    pub const SEED: &'static [u8] = b"consensus";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_attestor() {
        let addr1 = EthereumAddress::new([1u8; 20]);
        let addr2 = EthereumAddress::new([2u8; 20]);
        let addr3 = EthereumAddress::new([3u8; 20]);

        let client_state = ClientState {
            attestor_addresses: vec![addr1, addr2],
            min_required_sigs: 2,
            latest_height: 0,
            is_frozen: false,
        };

        assert!(client_state.is_attestor(&addr1));
        assert!(client_state.is_attestor(&addr2));
        assert!(!client_state.is_attestor(&addr3));
    }

    #[test]
    fn test_freeze() {
        let mut client_state = ClientState {
            attestor_addresses: vec![EthereumAddress::new([1u8; 20])],
            min_required_sigs: 1,
            latest_height: 100,
            is_frozen: false,
        };

        assert!(!client_state.is_frozen());
        client_state.freeze();
        assert!(client_state.is_frozen());
    }
}
