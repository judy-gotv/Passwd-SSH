//! Secret-holding primitive types with memory zeroization.
//! Author: Passwd Word

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use zeroize::Zeroize;

/// A heap-allocated byte buffer that is zeroized on drop.
///
/// Use this for passwords, private keys and passphrases while they are
/// in memory. Never log or `Debug`-print the contents.
#[derive(Clone)]
pub struct SecretBytes(Box<[u8]>);

impl SecretBytes {
    /// Copy bytes into a new secret container.
    pub fn new(bytes: impl AsRef<[u8]>) -> Self {
        Self(bytes.as_ref().to_vec().into_boxed_slice())
    }

    /// Borrow the secret bytes.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    /// Length of the secret in bytes (safe to reveal).
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the secret is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes([redacted])")
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Zeroize for SecretBytes {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl Serialize for SecretBytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Serialize as UTF-8 lossy string; only used for JSON export flows
        // where the caller explicitly opted in.
        serializer.serialize_str(&String::from_utf8_lossy(self.expose()))
    }
}

impl<'de> Deserialize<'de> for SecretBytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self::new(s.as_bytes()))
    }
}

/// A UTF-8 secret string backed by [`SecretBytes`].
#[derive(Clone)]
pub struct SecretString(SecretBytes);

impl SecretString {
    /// Create from a string-like value.
    pub fn new(s: impl AsRef<str>) -> Self {
        Self(SecretBytes::new(s.as_ref().as_bytes()))
    }

    /// Borrow the secret as `str`.
    ///
    /// # Panics
    /// Panics if the bytes are not valid UTF-8 (cannot happen when built
    /// via [`SecretString::new`]).
    pub fn expose(&self) -> &str {
        std::str::from_utf8(self.0.expose()).expect("SecretString holds valid UTF-8")
    }

    /// Borrow the raw bytes.
    pub fn expose_bytes(&self) -> &[u8] {
        self.0.expose()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString([redacted])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_leak() {
        let s = SecretString::new("hunter2-secret");
        let d = format!("{s:?}");
        assert!(!d.contains("hunter2"));
    }

    #[test]
    fn roundtrip_expose() {
        let s = SecretBytes::new(b"abc123");
        assert_eq!(s.expose(), b"abc123");
        assert_eq!(s.len(), 6);
    }
}
