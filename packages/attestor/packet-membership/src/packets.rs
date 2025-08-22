use alloy_primitives::FixedBytes as AlloyFixedBytes;
use alloy_sol_types::SolType;
use alloy_sol_types::sol_data::FixedBytes as SolFixedBytes;
use alloy_sol_types::sol_data::Array as SolArray;

/// Wrapper type that represents a list of packet commitments.
/// Each packet commitment is represented as a fixed-size 32-byte array.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Packets(Vec<AlloyFixedBytes<32>>);

impl Packets {
    /// Create a new instance of [Packets] from any type that can be converted to 32-byte fixed bytes
    #[must_use]
    pub fn new<T>(packets: Vec<T>) -> Self
    where
        T: Into<AlloyFixedBytes<32>>,
    {
        let packets: Vec<AlloyFixedBytes<32>> = packets.into_iter().map(Into::into).collect();
        Self(packets)
    }

    /// Iterate over each individual packet commitment
    pub fn packets(&self) -> impl Iterator<Item = &AlloyFixedBytes<32>> {
        self.0.iter()
    }
    
    /// Encode packet commitments to ABI bytes as bytes32[]
    pub fn to_abi_bytes(&self) -> Vec<u8> {
        SolArray::<SolFixedBytes<32>>::abi_encode(&self.0)
    }
    
    /// Decode packet commitments from ABI bytes encoded as bytes32[]
    pub fn from_abi_bytes(data: &[u8]) -> Result<Self, alloy_sol_types::Error> {
        let packets = SolArray::<SolFixedBytes<32>>::abi_decode(data)?;
        Ok(Self(packets))
    }
    
    /// Get the inner Vec<FixedBytes<32>>
    pub fn into_inner(self) -> Vec<AlloyFixedBytes<32>> {
        self.0
    }
    
    /// Get a reference to the inner Vec<FixedBytes<32>>
    pub fn as_inner(&self) -> &Vec<AlloyFixedBytes<32>> {
        &self.0
    }
}
