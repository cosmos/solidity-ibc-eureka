use alloy_primitives::FixedBytes;

use crate::adapter_client::Signable;

#[derive(Debug)]
pub struct Header {
    height: u64,
    state: FixedBytes<32>,
    timestamp: u64,
}

impl Header {
    pub fn new(height: u64, state: FixedBytes<32>, timestamp: u64) -> Self {
        Self {
            height,
            state,
            timestamp,
        }
    }
}

impl Signable for Header {}
