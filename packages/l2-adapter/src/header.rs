use alloy::primitives::FixedBytes;

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
