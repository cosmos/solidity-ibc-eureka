use serde::{Deserialize, Serialize};

/// Wrapper type that represents the serde byte-encoded
/// list of packets.
#[derive(Deserialize, Serialize, Clone)]
pub struct Packets(Vec<Vec<u8>>);

impl Packets {
    /// Create a new intances of [Packets]
    #[must_use]
    pub const fn new(packets: Vec<Vec<u8>>) -> Self {
        Self(packets)
    }

    /// Iterate over each individual packet
    pub fn packets(&self) -> impl Iterator<Item = &[u8]> {
        self.0.iter().map(std::vec::Vec::as_slice)
    }
}
