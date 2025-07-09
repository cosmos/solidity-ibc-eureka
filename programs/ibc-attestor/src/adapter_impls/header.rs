use alloy_primitives::FixedBytes;

use crate::adapter_client::Signable;

#[derive(Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Header {
    height: u64,
    state: [u8; 32],
    timestamp: u64,
}

impl Header {
    pub fn new(height: u64, state: FixedBytes<32>, timestamp: u64) -> Self {
        Self {
            height,
            state: state.into(),
            timestamp,
        }
    }
}

impl Signable for Header {
    fn height(&self) -> u64 {
        self.height
    }
}
