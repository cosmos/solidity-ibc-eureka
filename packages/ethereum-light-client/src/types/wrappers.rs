use alloy_primitives::{aliases::B32, Bloom, Bytes, FixedBytes, B256};
use serde::{Deserialize, Serialize};
use tree_hash::{MerkleHasher, TreeHash, BYTES_PER_CHUNK};

use super::bls::BlsPublicKey;

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct WrappedVersion(#[serde(with = "ethereum_utils::base64::fixed_size")] pub B32);

impl TreeHash for WrappedVersion {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        FixedBytes::tree_hash_type()
    }

    fn tree_hash_packed_encoding(&self) -> tree_hash::PackedEncoding {
        self.0.tree_hash_packed_encoding()
    }

    fn tree_hash_packing_factor() -> usize {
        FixedBytes::tree_hash_packing_factor()
    }

    fn tree_hash_root(&self) -> tree_hash::Hash256 {
        self.0.tree_hash_root()
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct WrappedBytes(#[serde(with = "ethereum_utils::base64")] pub Bytes);

impl TreeHash for WrappedBytes {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        tree_hash::TreeHashType::List
    }

    fn tree_hash_packed_encoding(&self) -> tree_hash::PackedEncoding {
        unreachable!("List should never be packed.")
    }

    fn tree_hash_packing_factor() -> usize {
        unreachable!("List should never be packed.")
    }

    fn tree_hash_root(&self) -> tree_hash::Hash256 {
        let leaves = self.0.len().div_ceil(BYTES_PER_CHUNK);

        let mut hasher = MerkleHasher::with_leaves(leaves);
        for item in &self.0 {
            hasher.write(item.tree_hash_root()[..1].as_ref()).unwrap()
        }

        tree_hash::mix_in_length(&hasher.finish().unwrap(), self.0.len())
    }
}

impl AsRef<[u8]> for WrappedBytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct WrappedBloom(#[serde(with = "ethereum_utils::base64::fixed_size")] pub Bloom);

impl TreeHash for WrappedBloom {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        tree_hash::TreeHashType::List
    }

    fn tree_hash_packed_encoding(&self) -> tree_hash::PackedEncoding {
        unreachable!("List should never be packed.")
    }

    fn tree_hash_packing_factor() -> usize {
        unreachable!("List should never be packed.")
    }

    fn tree_hash_root(&self) -> tree_hash::Hash256 {
        let leaves = self.0.len().div_ceil(BYTES_PER_CHUNK);

        let mut hasher = MerkleHasher::with_leaves(leaves);

        for item in &self.0 {
            hasher.write(item.tree_hash_root()[..1].as_ref()).unwrap()
        }

        hasher.finish().unwrap()
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct WrappedBranch<const N: usize>(
    #[serde(with = "ethereum_utils::base64::fixed_size::vec::fixed_size")] pub [B256; N],
);

impl<const N: usize> TreeHash for WrappedBranch<N> {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        tree_hash::TreeHashType::List
    }

    fn tree_hash_packed_encoding(&self) -> tree_hash::PackedEncoding {
        unreachable!("List should never be packed.")
    }

    fn tree_hash_packing_factor() -> usize {
        unreachable!("List should never be packed.")
    }

    fn tree_hash_root(&self) -> tree_hash::Hash256 {
        let leaves = self.0.len().div_ceil(BYTES_PER_CHUNK);
        let mut hasher = MerkleHasher::with_leaves(leaves);

        for item in &self.0 {
            hasher.write(item.tree_hash_root()[..1].as_ref()).unwrap()
        }

        hasher.finish().unwrap()
    }
}

impl<const N: usize> Default for WrappedBranch<N> {
    fn default() -> Self {
        Self([B256::default(); N])
    }
}

impl<const N: usize> From<WrappedBranch<N>> for Vec<B256> {
    fn from(val: WrappedBranch<N>) -> Self {
        val.0.to_vec()
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct WrappedVecBlsPublicKey(
    #[serde(with = "ethereum_utils::base64::fixed_size::vec")] pub Vec<BlsPublicKey>,
);

impl TreeHash for WrappedVecBlsPublicKey {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        tree_hash::TreeHashType::Vector
    }

    fn tree_hash_packed_encoding(&self) -> tree_hash::PackedEncoding {
        unreachable!("Vector should never be packed.")
    }

    fn tree_hash_packing_factor() -> usize {
        unreachable!("Vector should never be packed.")
    }

    fn tree_hash_root(&self) -> tree_hash::Hash256 {
        let leaves = self.0.len();
        let mut hasher = MerkleHasher::with_leaves(leaves);

        for item in &self.0 {
            hasher.write(item.tree_hash_root().as_ref()).unwrap()
        }

        hasher.finish().unwrap()
    }
}
