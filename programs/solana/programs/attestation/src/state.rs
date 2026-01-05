use anchor_lang::prelude::*;

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
    /// Fixed list of attestor addresses. Maximum of 10 attestors supported
    #[max_len(10)]
    pub attestor_addresses: Vec<Pubkey>,

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

    /// Check if an address is in the attestor set
    pub fn is_attestor(&self, address: &Pubkey) -> bool {
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
        let addr1 = Pubkey::new_unique();
        let addr2 = Pubkey::new_unique();
        let addr3 = Pubkey::new_unique();

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
            attestor_addresses: vec![Pubkey::new_unique()],
            min_required_sigs: 1,
            latest_height: 100,
            is_frozen: false,
        };

        assert!(!client_state.is_frozen());
        client_state.freeze();
        assert!(client_state.is_frozen());
    }
}
