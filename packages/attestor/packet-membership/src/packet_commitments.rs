use alloy_primitives::FixedBytes as AlloyFixedBytes;
use alloy_sol_types::SolValue;

/// handy alias for 32-byte fixed bytes
type B32 = AlloyFixedBytes<32>;

/// Represents lightweight packet as hash(path.path()) && packet.commitment().
/// Including path hash implies replay-protection for attestations
/// (because we can't rely on a merkle proof)
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PacketCompact {
    /// Packet's `commitment_path` hash
    pub path: B32,

    /// Packet's `commitment` hash
    pub commitment: B32,
}

impl PacketCompact {
    /// Create a new packet compact from a path and a commitment
    pub fn new<T>(path: T, commitment: T) -> Self
    where
        T: Into<B32>,
    {
        Self {
            path: path.into(),
            commitment: commitment.into(),
        }
    }

    /// Create a new packet compact from a tuple of path and commitment
    pub fn new_from_tuple<T>(tuple: (T, T)) -> Self
    where
        T: Into<B32>,
    {
        Self::new(tuple.0, tuple.1)
    }

    /// Convert packet compact to a tuple of path and commitment
    #[inline]
    pub fn as_tuple(&self) -> (B32, B32) {
        (self.path, self.commitment)
    }

    /// Encode packet compact as tuple(path_hash, commitment_hash)
    pub fn to_abi_bytes(&self) -> Vec<u8> {
        self.as_tuple().abi_encode()
    }

    /// Decode packet compact from ABI bytes encoded as tuple(bytes32, bytes32)
    pub fn from_abi_bytes(raw: &[u8]) -> Result<Self, alloy_sol_types::Error> {
        let (path, commitment) = <(B32, B32)>::abi_decode(raw)?;

        Ok(Self { path, commitment })
    }
}

/// Wrapper type that represents a list of packet commitments.
/// Each packet commitment is represented as a fixed-size 32-byte array.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PacketCommitments(Vec<PacketCompact>);

impl PacketCommitments {
    /// Create a new instance of [Packets] from a vector of [PacketCompact]
    #[must_use]
    pub fn new(packets: Vec<PacketCompact>) -> Self {
        Self(packets)
    }

    /// Iterate over each individual packet commitment
    #[must_use]
    pub fn iterate(&self) -> impl Iterator<Item = &PacketCompact> {
        self.0.iter()
    }

    /// Iterate over each individual packet commitment
    #[must_use]
    pub fn commitments(&self) -> impl Iterator<Item = &B32> {
        self.0.iter().map(|p| &p.commitment)
    }

    /// Encode packet commitments to ABI bytes as (bytes32,bytes32)[]
    #[must_use]
    pub fn to_abi_bytes(&self) -> Vec<u8> {
        self.iterate()
            .map(PacketCompact::as_tuple)
            .collect::<Vec<_>>()
            .abi_encode()
    }

    /// Decode packet commitments from ABI bytes encoded as (bytes32,bytes32)[]
    pub fn from_abi_bytes(raw: &[u8]) -> Result<Self, alloy_sol_types::Error> {
        let tuples: Vec<(B32, B32)> = Vec::<(B32, B32)>::abi_decode(raw)?;
        let packets = tuples
            .into_iter()
            .map(PacketCompact::new_from_tuple)
            .collect();

        Ok(Self(packets))
    }

    /// Convert to inner vector of [PacketCompact]
    pub fn into_inner(self) -> Vec<PacketCompact> {
        self.0
    }

    /// Get inner vector of [PacketCompact]
    pub const fn as_inner(&self) -> &Vec<PacketCompact> {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_commitments_roundtrip() {
        // ARRANGE
        // Given a sample ABI-encoded hex
        //
        // cast abi-encode "fn((bytes32,bytes32)[])" \
        //  "[(0x1000000000000000000000000000000000000000000000000000000000000000,0x2000000000000000000000000000000000000000000000000000000000000000), \
        //    (0x3000000000000000000000000000000000000000000000000000000000000000,0x4000000000000000000000000000000000000000000000000000000000000000)]"
        const SAMPLE_HEX: &str = concat!(
            "0x00000000000000000000000000000000000000000000000000000000000000",
            "200000000000000000000000000000000000000000000000000000000000000002",
            "1000000000000000000000000000000000000000000000000000000000000000",
            "2000000000000000000000000000000000000000000000000000000000000000",
            "3000000000000000000000000000000000000000000000000000000000000000",
            "4000000000000000000000000000000000000000000000000000000000000000",
        );

        let sample_bytes = hex::decode(&SAMPLE_HEX[2..]).expect("Invalid hex");

        // ACT #1
        // Decode the hex string to bytes
        let res = PacketCommitments::from_abi_bytes(&sample_bytes);

        // ASSERT #1
        assert!(
            res.is_ok(),
            "Failed to parse PacketCommitments: {:?}",
            res.unwrap_err()
        );

        let packets = res.unwrap();

        // Check that the number of packets is correct
        assert_eq!(packets.iterate().count(), 2, "Expected 2 packets");

        // Check that tuple order is preserved
        // just check first packet is  (0x10..., 0x20...)
        let first = packets.as_inner().get(0).unwrap();
        assert_eq!(first.path.as_slice()[0], 0x10);
        assert_eq!(first.commitment.as_slice()[0], 0x20);

        // ACT #2
        // Encode back to ABI bytes
        let encoded_bytes = packets.to_abi_bytes();

        // Convert back to hex string
        let encoded_hex = format!("0x{}", hex::encode(&encoded_bytes));

        // ASSERT #2
        assert_eq!(encoded_hex, SAMPLE_HEX, "hex mismatch");
    }
}
