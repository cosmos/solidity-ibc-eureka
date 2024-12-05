use super::*;
use alloy_primitives::{Address, B256, U128, U256};
use std::sync::Arc;

fn int_to_hash256(int: u64) -> Hash256 {
    let mut bytes = [0; HASHSIZE];
    bytes[0..8].copy_from_slice(&int.to_le_bytes());
    Hash256::from_slice(&bytes)
}

macro_rules! impl_for_bitsize {
    ($type: ident, $bit_size: expr) => {
        impl TreeHash for $type {
            fn tree_hash_type() -> TreeHashType {
                TreeHashType::Basic
            }

            fn tree_hash_packed_encoding(&self) -> PackedEncoding {
                PackedEncoding::from_slice(&self.to_le_bytes())
            }

            fn tree_hash_packing_factor() -> usize {
                HASHSIZE / ($bit_size / 8)
            }

            #[allow(clippy::cast_lossless)] // Lint does not apply to all uses of this macro.
            fn tree_hash_root(&self) -> Hash256 {
                int_to_hash256(*self as u64)
            }
        }
    };
}

impl_for_bitsize!(u8, 8);
impl_for_bitsize!(u16, 16);
impl_for_bitsize!(u32, 32);
impl_for_bitsize!(u64, 64);
impl_for_bitsize!(usize, 64);

impl TreeHash for bool {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Basic
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        (*self as u8).tree_hash_packed_encoding()
    }

    fn tree_hash_packing_factor() -> usize {
        u8::tree_hash_packing_factor()
    }

    fn tree_hash_root(&self) -> Hash256 {
        int_to_hash256(*self as u64)
    }
}

/// Only valid for byte types less than 32 bytes.
macro_rules! impl_for_lt_32byte_u8_array {
    ($len: expr) => {
        impl TreeHash for [u8; $len] {
            fn tree_hash_type() -> TreeHashType {
                TreeHashType::Vector
            }

            fn tree_hash_packed_encoding(&self) -> PackedEncoding {
                unreachable!("bytesN should never be packed.")
            }

            fn tree_hash_packing_factor() -> usize {
                unreachable!("bytesN should never be packed.")
            }

            fn tree_hash_root(&self) -> Hash256 {
                let mut result = [0; 32];
                result[0..$len].copy_from_slice(&self[..]);
                Hash256::from_slice(&result)
            }
        }
    };
}

impl_for_lt_32byte_u8_array!(4);
impl_for_lt_32byte_u8_array!(32);

impl TreeHash for [u8; 48] {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Vector
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        unreachable!("Vector should never be packed.")
    }

    fn tree_hash_packing_factor() -> usize {
        unreachable!("Vector should never be packed.")
    }

    fn tree_hash_root(&self) -> Hash256 {
        let values_per_chunk = BYTES_PER_CHUNK;
        let minimum_chunk_count = (48 + values_per_chunk - 1) / values_per_chunk;
        merkle_root(self, minimum_chunk_count)
    }
}

impl TreeHash for U128 {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Basic
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        PackedEncoding::from_slice(&self.to_le_bytes::<{ Self::BYTES }>())
    }

    fn tree_hash_packing_factor() -> usize {
        2
    }

    fn tree_hash_root(&self) -> Hash256 {
        Hash256::right_padding_from(&self.to_le_bytes::<{ Self::BYTES }>())
    }
}

impl TreeHash for U256 {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Basic
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        PackedEncoding::from(self.to_le_bytes::<{ Self::BYTES }>())
    }

    fn tree_hash_packing_factor() -> usize {
        1
    }

    fn tree_hash_root(&self) -> Hash256 {
        Hash256::from(self.to_le_bytes::<{ Self::BYTES }>())
    }
}

impl TreeHash for Address {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Vector
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        let mut result = [0; 32];
        result[0..20].copy_from_slice(self.as_slice());
        PackedEncoding::from_slice(&result)
    }

    fn tree_hash_packing_factor() -> usize {
        1
    }

    fn tree_hash_root(&self) -> Hash256 {
        let mut result = [0; 32];
        result[0..20].copy_from_slice(self.as_slice());
        Hash256::from_slice(&result)
    }
}

impl TreeHash for B256 {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Vector
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        PackedEncoding::from_slice(self.as_slice())
    }

    fn tree_hash_packing_factor() -> usize {
        1
    }

    fn tree_hash_root(&self) -> Hash256 {
        *self
    }
}

impl<T: TreeHash> TreeHash for Arc<T> {
    fn tree_hash_type() -> TreeHashType {
        T::tree_hash_type()
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        self.as_ref().tree_hash_packed_encoding()
    }

    fn tree_hash_packing_factor() -> usize {
        T::tree_hash_packing_factor()
    }

    fn tree_hash_root(&self) -> Hash256 {
        self.as_ref().tree_hash_root()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn bool() {
        let mut true_bytes: Vec<u8> = vec![1];
        true_bytes.append(&mut vec![0; 31]);

        let false_bytes: Vec<u8> = vec![0; 32];

        assert_eq!(true.tree_hash_root().as_slice(), true_bytes.as_slice());
        assert_eq!(false.tree_hash_root().as_slice(), false_bytes.as_slice());
    }

    #[test]
    fn arc() {
        let one = U128::from(1);
        let one_arc = Arc::new(one);
        assert_eq!(one_arc.tree_hash_root(), one.tree_hash_root());
    }

    #[test]
    fn int_to_bytes() {
        assert_eq!(int_to_hash256(0).as_slice(), &[0; 32]);
        assert_eq!(
            int_to_hash256(1).as_slice(),
            &[
                1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0
            ]
        );
        assert_eq!(
            int_to_hash256(u64::max_value()).as_slice(),
            &[
                255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
    }
}
