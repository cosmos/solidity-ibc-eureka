use alloy_primitives::hex;
use serde::{de, Deserialize, Deserializer, Serializer};

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

pub mod vec {
    use alloy_primitives::hex;
    use serde::{de, Deserialize, Deserializer, Serializer};

    use super::to_hex;

    pub fn serialize<S: Serializer, T: AsRef<[u8]>, const VEC_SIZE: usize>(
        #[allow(clippy::ptr_arg)] // required by serde
        bytes: &[T; VEC_SIZE],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(bytes.iter().map(to_hex))
    }

    pub fn deserialize<'de, D, T, const VEC_SIZE: usize, const TYPE_SIZE: usize>(
        deserializer: D,
    ) -> Result<[T; VEC_SIZE], D::Error>
    where
        D: Deserializer<'de>,
        T: TryFrom<[u8; TYPE_SIZE]>,
    {
        let vec = Vec::<String>::deserialize(deserializer)?;
        let items: Vec<T> = vec
            .into_iter()
            .map(|s| {
                let decoded = hex::decode(&s).map_err(de::Error::custom)?;
                let fixed_sized: [u8; TYPE_SIZE] = decoded
                    .as_slice()
                    .try_into()
                    .map_err(|_| de::Error::custom("Invalid hex data"))?;
                T::try_from(fixed_sized).map_err(|_| de::Error::custom("Invalid hex data"))
            })
            .collect::<Result<Vec<T>, _>>()?;

        items
            .try_into()
            .map_err(|_| de::Error::custom("Invalid base64 data"))
    }
}

pub struct HexVec;

impl HexVec {
    pub fn serialize<S: Serializer, T: AsRef<[u8]>, const VEC_SIZE: usize>(
        #[allow(clippy::ptr_arg)] // required by serde
        bytes: &[T; VEC_SIZE],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(bytes.iter().map(to_hex))
    }

    pub fn deserialize<'de, D, T, const VEC_SIZE: usize, const TYPE_SIZE: usize>(
        deserializer: D,
    ) -> Result<[T; VEC_SIZE], D::Error>
    where
        D: Deserializer<'de>,
        T: TryFrom<[u8; TYPE_SIZE]>,
    {
        let vec = Vec::<String>::deserialize(deserializer)?;
        let items: Vec<T> = vec
            .into_iter()
            .map(|s| {
                let decoded = hex::decode(&s).map_err(de::Error::custom)?;
                let fixed_sized: [u8; TYPE_SIZE] = decoded
                    .as_slice()
                    .try_into()
                    .map_err(|_| de::Error::custom("Invalid hex data"))?;
                T::try_from(fixed_sized).map_err(|_| de::Error::custom("Invalid hex data"))
            })
            .collect::<Result<Vec<T>, _>>()?;

        items
            .try_into()
            .map_err(|_| de::Error::custom("Invalid base64 data"))
    }
}
