//! Master-password vault: Argon2id key derivation + ChaCha20-Poly1305 AEAD.
//!
//! Design (see docs §7.1):
//! - The user picks a master password.
//! - `Argon2id` derives a 256-bit key from the password and a random salt.
//! - Credentials are encrypted with `ChaCha20-Poly1305` (random 96-bit nonce
//!   per encryption).
//! - Only ciphertext + nonce + salt are persisted. The derived key lives in
//!   memory and is zeroized on lock/drop.
//! - A fixed "check" plaintext encrypted at creation time lets us detect a
//!   wrong master password on unlock.
//!
//! Author: Passwd Word

use argon2::{Argon2, Params};
use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit,
    aead::{Aead, Key, Nonce},
};
use rand::{RngCore, rngs::OsRng};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::{SecretString, VaultError};

/// Length of the derived encryption key in bytes.
const KEY_LEN: usize = 32;
/// Length of the ChaCha20-Poly1305 nonce in bytes.
const NONCE_LEN: usize = 12;
/// Length of the Argon2 salt in bytes.
const SALT_LEN: usize = 32;

/// Magic plaintext encrypted at vault creation; decrypting it successfully
/// proves the master password is correct.
const CHECK_PLAINTEXT: &[u8] = b"opsdeck-vault-check-v1";

/// An encrypted data blob: nonce + ciphertext.
///
/// Only this (plus the [`VaultVerifier`]) is ever written to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedBlob {
    /// 96-bit nonce, unique per encryption.
    pub nonce: [u8; NONCE_LEN],
    /// ChaCha20-Poly1305 ciphertext (includes the 16-byte auth tag).
    pub ciphertext: Vec<u8>,
}

/// Persisted vault parameters: salt + an encrypted check value.
///
/// Store this (e.g. in the app config file) to allow future unlocks.
/// It contains no secret material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultVerifier {
    /// Argon2 salt.
    pub salt: [u8; SALT_LEN],
    /// Encrypted [`CHECK_PLAINTEXT`]; proves the password on unlock.
    pub check: EncryptedBlob,
    /// KDF parameters, so future versions can migrate gracefully.
    pub kdf: KdfParams,
}

/// KDF parameters used for key derivation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdfParams {
    pub algorithm: String,
    pub version: u32,
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        let p = Params::default();
        Self {
            algorithm: "argon2id".to_string(),
            version: 0x13,
            m_cost_kib: p.m_cost(),
            t_cost: p.t_cost(),
            parallelism: p.p_cost(),
        }
    }
}

/// An unlocked vault: holds the derived encryption key in memory.
///
/// The key is zeroized when the vault is locked/dropped. While unlocked,
/// callers can encrypt/decrypt credential material.
pub struct Vault {
    key: [u8; KEY_LEN],
}

impl Vault {
    /// Create a brand-new vault from a master password.
    ///
    /// Returns the unlocked vault plus a [`VaultVerifier`] to persist.
    pub fn create(master: &SecretString) -> Result<(Self, VaultVerifier), VaultError> {
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);

        let kdf = KdfParams::default();
        let key = derive_key(master.expose_secret().as_bytes(), &salt, &kdf)?;
        let vault = Self { key };

        let check = vault.encrypt(CHECK_PLAINTEXT)?;
        let verifier = VaultVerifier { salt, check, kdf };
        Ok((vault, verifier))
    }

    /// Unlock an existing vault, verifying the master password.
    ///
    /// Returns [`VaultError::WrongPassword`] when the password is incorrect
    /// (or the verifier is corrupt — indistinguishable by design).
    pub fn unlock(
        master: &SecretString,
        verifier: &VaultVerifier,
    ) -> Result<Self, VaultError> {
        let key = derive_key(master.expose_secret().as_bytes(), &verifier.salt, &verifier.kdf)?;
        let vault = Self { key };
        let check = vault.decrypt(&verifier.check).map_err(|_| VaultError::WrongPassword)?;
        if check != CHECK_PLAINTEXT {
            return Err(VaultError::WrongPassword);
        }
        Ok(vault)
    }

    /// Encrypt plaintext with a fresh random nonce.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedBlob, VaultError> {
        let cipher = cipher_from_key(&self.key);
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::<ChaCha20Poly1305>::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| VaultError::Encryption)?;
        Ok(EncryptedBlob { nonce: nonce_bytes, ciphertext })
    }

    /// Decrypt a blob. Fails when the blob was tampered with.
    pub fn decrypt(&self, blob: &EncryptedBlob) -> Result<Vec<u8>, VaultError> {
        let cipher = cipher_from_key(&self.key);
        let nonce = Nonce::<ChaCha20Poly1305>::from_slice(&blob.nonce);
        cipher
            .decrypt(nonce, blob.ciphertext.as_slice())
            .map_err(|_| VaultError::InvalidBlob)
    }

    /// Derive a new vault under a new master password.
    ///
    /// Returns the new vault + verifier. The caller must re-encrypt every
    /// stored credential: decrypt with `self`, encrypt with the new vault.
    /// Only call this on an already-unlocked vault.
    pub fn rekey(&self, new_master: &SecretString) -> Result<(Self, VaultVerifier), VaultError> {
        Self::create(new_master)
    }

    /// Lock the vault, zeroizing the key material.
    pub fn lock(mut self) {
        self.key.zeroize();
    }
}

impl Drop for Vault {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

fn derive_key(password: &[u8], salt: &[u8], kdf: &KdfParams) -> Result<[u8; KEY_LEN], VaultError> {
    let params = Params::new(kdf.m_cost_kib, kdf.t_cost, kdf.parallelism, Some(KEY_LEN))
        .map_err(|_| VaultError::KeyDerivation)?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(password, salt, &mut key)
        .map_err(|_| VaultError::KeyDerivation)?;
    Ok(key)
}

fn cipher_from_key(key: &[u8; KEY_LEN]) -> ChaCha20Poly1305 {
    let key = Key::<ChaCha20Poly1305>::from_slice(key);
    ChaCha20Poly1305::new(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_vault() -> (Vault, VaultVerifier) {
        Vault::create(&SecretString::new("correct horse battery staple")).unwrap()
    }

    #[test]
    fn create_and_unlock_roundtrip() {
        let (vault, verifier) = test_vault();
        let blob = vault.encrypt(b"my-secret-password").unwrap();
        let back = vault.decrypt(&blob).unwrap();
        assert_eq!(back, b"my-secret-password");

        let vault2 = Vault::unlock(&SecretString::new("correct horse battery staple"), &verifier).unwrap();
        let back2 = vault2.decrypt(&blob).unwrap();
        assert_eq!(back2, b"my-secret-password");
    }

    #[test]
    fn wrong_password_rejected() {
        let (_vault, verifier) = test_vault();
        let err = Vault::unlock(&SecretString::new("wrong password"), &verifier).unwrap_err();
        assert!(matches!(err, VaultError::WrongPassword));
    }

    #[test]
    fn nonces_are_unique() {
        let (vault, _v) = test_vault();
        let a = vault.encrypt(b"same plaintext").unwrap();
        let b = vault.encrypt(b"same plaintext").unwrap();
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.ciphertext, b.ciphertext);
    }

    #[test]
    fn tampered_blob_rejected() {
        let (vault, _v) = test_vault();
        let mut blob = vault.encrypt(b"data").unwrap();
        let last = blob.ciphertext.len() - 1;
        blob.ciphertext[last] ^= 0xff;
        assert!(matches!(vault.decrypt(&blob), Err(VaultError::InvalidBlob)));
    }

    #[test]
    fn rekey_keeps_working() {
        let (vault, _v) = test_vault();
        let (vault2, verifier2) =
            vault.rekey(&SecretString::new("brand new password")).unwrap();
        let blob = vault2.encrypt(b"hello").unwrap();
        let v3 = Vault::unlock(&SecretString::new("brand new password"), &verifier2).unwrap();
        assert_eq!(v3.decrypt(&blob).unwrap(), b"hello");
        // Old password must not open the new verifier.
        assert!(Vault::unlock(&SecretString::new("correct horse battery staple"), &verifier2).is_err());
    }
}
