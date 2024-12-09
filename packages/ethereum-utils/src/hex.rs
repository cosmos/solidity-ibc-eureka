use alloy_primitives::{hex, U256};

// TODO: Unit test
pub fn to_hex<T: AsRef<[u8]>>(data: T) -> String {
    let data = data.as_ref();

    let encoded = if data.is_empty() {
        "0".to_string()
    } else {
        hex::encode(data)
    };

    format!("0x{encoded}")
}

pub trait FromBeHex {
    fn from_be_hex(hex_str: &str) -> Self;
}

// TODO: Unit test sad paths (i.e. should this check for valid be?)
impl FromBeHex for U256 {
    fn from_be_hex(hex_str: &str) -> Self {
        let data = hex::decode(hex_str).unwrap();
        U256::from_be_slice(data.as_slice())
    }
}

#[cfg(test)]
mod test {
    use alloy_primitives::{hex::FromHex, B256, U256};

    use super::FromBeHex;

    #[test]
    // This is primarily to document how to convert the alloy primitives to and from hex
    fn test_alloy_primitive_hex() {
        // From hex string to B256
        let expected_hex = "0x75d7411cb01daad167713b5a9b7219670f0e500653cbbcd45cfe1bfe04222459";
        let b256 = B256::from_hex(expected_hex).unwrap();
        let b256_hex = format!("0x{b256:x}");
        assert_eq!(expected_hex, b256_hex);

        // From hex string to U256
        let u256 = U256::from_be_hex(expected_hex);
        let u256_hex = format!("0x{u256:x}");
        assert_eq!(expected_hex, u256_hex);

        // From B256 to U256 to hex string
        let u256: U256 = b256.into();
        let u256_hex = format!("0x{u256:x}");
        assert_eq!(expected_hex, u256_hex);
    }

    #[test]
    fn test_to_be_hex() {
        let be_hex_str = "0x0000000000000000000000000000000000000000000000000000000000000001";
        let u256 = U256::from_be_hex(be_hex_str);
        let num: u64 = u256.to_base_le(10).reduce(|acc, n| acc + n).unwrap();
        assert_eq!(1, num);
    }
}
