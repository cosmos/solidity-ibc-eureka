//! This module provides custom serde implementations.

/// Serialize a number as a string.
pub mod number_as_string {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Implements the serde `serialize` function for a number.
    /// # Errors
    /// Returns an error if the number cannot be serialized.
    /// # Returns
    /// Returns if the number is serialized as a string successfully.
    pub fn serialize<T, S>(number: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: ToString,
        S: Serializer,
    {
        serializer.serialize_str(&number.to_string())
    }

    /// Implements the serde `deserialize` function for a number.
    /// # Errors
    /// Returns an error if the string cannot be deserialized to a number.
    /// # Returns
    /// Returns the number deserialized from a string.
    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
