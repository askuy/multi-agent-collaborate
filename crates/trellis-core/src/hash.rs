use std::fmt;
use std::str::FromStr;

use crate::error::{CoreError, Result};

/// A BLAKE3-256 content hash.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash([u8; 32]);

impl Hash {
    /// Wraps raw hash bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Hashes `data` with BLAKE3.
    pub fn of(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }

    /// Reads a hash from a slice that must be exactly 32 bytes long.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let array: [u8; 32] = bytes.try_into().map_err(|_| {
            CoreError::InvalidHash(format!("expected 32 bytes, got {}", bytes.len()))
        })?;
        Ok(Self(array))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", &self.to_hex()[..12])
    }
}

impl FromStr for Hash {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self> {
        let bytes = hex::decode(s).map_err(|e| CoreError::InvalidHash(e.to_string()))?;
        Self::from_slice(&bytes)
    }
}

impl serde::Serialize for Hash {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> serde::Deserialize<'de> for Hash {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = <String as serde::Deserialize>::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_of_is_blake3_and_round_trips_through_hex() {
        let h = Hash::of(b"hello");
        assert_eq!(h.to_hex(), blake3::hash(b"hello").to_hex().to_string());
        assert_eq!(h.to_string(), h.to_hex());
        assert_eq!(h.to_hex().parse::<Hash>().unwrap(), h);
        assert_eq!(Hash::from_slice(h.as_bytes()).unwrap(), h);
    }

    #[test]
    fn rejects_wrong_length_and_bad_hex() {
        assert!(matches!(
            "abcd".parse::<Hash>(),
            Err(CoreError::InvalidHash(_))
        ));
        assert!(matches!(
            "zz".parse::<Hash>(),
            Err(CoreError::InvalidHash(_))
        ));
        assert!(Hash::from_slice(&[0u8; 31]).is_err());
    }
}
