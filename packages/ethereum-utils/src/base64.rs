use base64::prelude::*;
use serde::{de, Deserialize, Deserializer};

pub fn serialize<S, T: AsRef<[u8]>>(data: T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&BASE64_STANDARD.encode(data))
}

pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: TryFrom<Vec<u8>>,
{
    let s = String::deserialize(deserializer)?;
    let decoded = BASE64_STANDARD
        .decode(s.as_bytes())
        .map_err(de::Error::custom)?;
    T::try_from(decoded).map_err(|_| de::Error::custom("Invalid base64 data"))
}

pub mod fixed_size {
    use base64::prelude::*;
    use serde::{de, Deserialize, Deserializer};

    pub fn serialize<S, T: AsRef<[u8]>>(data: T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&BASE64_STANDARD.encode(data))
    }

    pub fn deserialize<'de, D, T, const N: usize>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: TryFrom<[u8; N]>,
    {
        let s = String::deserialize(deserializer)?;
        let decoded = BASE64_STANDARD
            .decode(s.as_bytes())
            .map_err(de::Error::custom)?;

        let fixed_sized: [u8; N] = decoded
            .as_slice()
            .try_into()
            .map_err(|_| de::Error::custom("Invalid base64 data"))?;
        T::try_from(fixed_sized).map_err(|_| de::Error::custom("Invalid base64 data"))
    }

    pub mod vec {
        use base64::prelude::*;
        use serde::{de, Deserialize, Deserializer, Serializer};

        pub fn serialize<S: Serializer, T: AsRef<[u8]>>(
            #[allow(clippy::ptr_arg)] // required by serde
            bytes: &Vec<T>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            serializer.collect_seq(bytes.iter().map(|b| BASE64_STANDARD.encode(b)))
        }

        pub fn deserialize<'de, D, T, const N: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
        where
            D: Deserializer<'de>,
            T: TryFrom<[u8; N]>,
        {
            let vec = Vec::<String>::deserialize(deserializer)?;
            vec.into_iter()
                .map(|s| {
                    let decoded = BASE64_STANDARD
                        .decode(s.as_bytes())
                        .map_err(de::Error::custom)?;
                    let fixed_sized: [u8; N] = decoded
                        .as_slice()
                        .try_into()
                        .map_err(|_| de::Error::custom("Invalid base64 data"))?;
                    T::try_from(fixed_sized).map_err(|_| de::Error::custom("Invalid base64 data"))
                })
                .collect()
        }

        pub mod fixed_size {
            use base64::prelude::*;
            use serde::{de, Deserialize, Deserializer, Serializer};

            pub fn serialize<S: Serializer, T: AsRef<[u8]>, const NN: usize>(
                #[allow(clippy::ptr_arg)] // required by serde
                bytes: &[T; NN],
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.collect_seq(bytes.iter().map(|b| BASE64_STANDARD.encode(b)))
            }

            pub fn deserialize<'de, D, T, const N: usize, const NN: usize>(
                deserializer: D,
            ) -> Result<[T; NN], D::Error>
            where
                D: Deserializer<'de>,
                T: TryFrom<[u8; N]>,
            {
                let vec = Vec::<String>::deserialize(deserializer)?;
                let items: Vec<T> = vec
                    .into_iter()
                    .map(|s| {
                        let decoded = BASE64_STANDARD
                            .decode(s.as_bytes())
                            .map_err(de::Error::custom)?;
                        let fixed_sized: [u8; N] = decoded
                            .as_slice()
                            .try_into()
                            .map_err(|_| de::Error::custom("Invalid base64 data"))?;
                        T::try_from(fixed_sized)
                            .map_err(|_| de::Error::custom("Invalid base64 data"))
                    })
                    .collect::<Result<Vec<T>, _>>()?;

                items
                    .try_into()
                    .map_err(|_| de::Error::custom("Invalid base64 data"))
            }
        }

        pub mod fixed_size_with_option {
            use base64::prelude::*;
            use serde::{de, Deserialize, Deserializer, Serializer};

            pub fn serialize<S: Serializer, T: AsRef<[u8]>, const NN: usize>(
                #[allow(clippy::ptr_arg)] // required by serde
                bytes: &Option<[T; NN]>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                if let Some(bytes) = bytes {
                    serializer.collect_seq(bytes.iter().map(|b| BASE64_STANDARD.encode(b)))
                } else {
                    serializer.collect_seq(None::<String>)
                }
            }

            pub fn deserialize<'de, D, T, const N: usize, const NN: usize>(
                deserializer: D,
            ) -> Result<Option<[T; NN]>, D::Error>
            where
                D: Deserializer<'de>,
                T: TryFrom<[u8; N]>,
            {
                let vec = Vec::<String>::deserialize(deserializer)?;
                if vec.is_empty() {
                    return Ok(None);
                }
                let items: Vec<T> = vec
                    .into_iter()
                    .map(|s| {
                        let decoded = BASE64_STANDARD
                            .decode(s.as_bytes())
                            .map_err(de::Error::custom)?;
                        let fixed_sized: [u8; N] = decoded
                            .as_slice()
                            .try_into()
                            .map_err(|_| de::Error::custom("Invalid base64 data"))?;
                        T::try_from(fixed_sized)
                            .map_err(|_| de::Error::custom("Invalid base64 data"))
                    })
                    .collect::<Result<Vec<T>, _>>()?;

                let data = items
                    .try_into()
                    .map_err(|_| de::Error::custom("Invalid base64 data"))?;

                Ok(Some(data))
            }
        }
    }
}

pub mod uint256 {
    use alloy_primitives::U256;
    use base64::{prelude::BASE64_STANDARD, Engine};
    use serde::{de, Deserialize, Deserializer};

    pub fn serialize<S>(data: &U256, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&BASE64_STANDARD.encode(data.to_be_bytes_vec()))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<U256, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let decoded = BASE64_STANDARD
            .decode(s.as_bytes())
            .map_err(de::Error::custom)?;

        Ok(U256::from_be_slice(decoded.as_slice()))
    }
}

pub mod bytes {
    use alloy_primitives::Bytes;
    use base64::{prelude::BASE64_STANDARD, Engine};
    use serde::{de, Deserialize, Deserializer};

    pub fn serialize<S>(data: &Bytes, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&BASE64_STANDARD.encode(data))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Bytes, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let decoded = BASE64_STANDARD
            .decode(s.as_bytes())
            .map_err(de::Error::custom)?;
        Ok(Bytes::from(decoded))
    }
}

pub mod option_with_default {
    use base64::{prelude::BASE64_STANDARD, Engine};
    use serde::{de, Deserialize, Deserializer};

    pub fn serialize<S, T: AsRef<[u8]>>(data: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(data) = data {
            serializer.serialize_str(&BASE64_STANDARD.encode(data))
        } else {
            serializer.serialize_str("")
        }
    }

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        for<'a> T: TryFrom<&'a [u8]>,
    {
        let s = String::deserialize(deserializer)?;
        if s.is_empty() {
            return Ok(None);
        }

        let decoded = BASE64_STANDARD
            .decode(s.as_bytes())
            .map_err(de::Error::custom)?;

        let data: T = T::try_from(decoded.as_slice())
            .map_err(|_| de::Error::custom("Invalid base64 data"))?;

        Ok(Some(data))
    }
}
