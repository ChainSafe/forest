// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fmt::Display,
    fs::{File, create_dir},
    io::{BufReader, BufWriter, ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

use crate::{
    shim::crypto::SignatureType,
    utils::{encoding::from_slice_with_fallback, io::create_new_sensitive_file},
};
use ahash::{HashMap, HashMapExt};
use argon2::{
    Argon2, ParamsBuilder, PasswordHasher, RECOMMENDED_SALT_LEN, password_hash::SaltString,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use crypto_secretbox::{
    KeyInit, SecretBox, XSalsa20Poly1305,
    aead::{Aead, generic_array::GenericArray},
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, warn};

use super::errors::Error;

const NONCE_SIZE: usize = SecretBox::<Box<dyn std::any::Any>>::NONCE_SIZE;

pub const KEYSTORE_NAME: &str = "keystore.json";
pub const ENCRYPTED_KEYSTORE_NAME: &str = "keystore";

/// Environmental variable which holds the `KeyStore` encryption phrase.
pub const FOREST_KEYSTORE_PHRASE_ENV: &str = "FOREST_KEYSTORE_PHRASE";

type SaltByteArray = [u8; RECOMMENDED_SALT_LEN];

/// `KeyInfo` structure, this contains the type of key (stored as a string) and
/// the private key. Note how the private key is stored as a byte vector
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[derive(Clone, PartialEq, Debug, Eq, Serialize, Deserialize, derive_more::Constructor)]
pub struct KeyInfo {
    key_type: SignatureType,
    // Vec<u8> is used because The private keys for BLS and SECP256K1 are not of the same type
    private_key: Vec<u8>,
}

#[derive(Clone, PartialEq, Debug, Eq, Serialize, Deserialize)]
pub struct PersistentKeyInfo {
    key_type: SignatureType,
    private_key: String,
}

impl KeyInfo {
    /// Return a reference to the key's signature type
    pub fn key_type(&self) -> &SignatureType {
        &self.key_type
    }

    /// Return a reference to the private key
    pub fn private_key(&self) -> &Vec<u8> {
        &self.private_key
    }
}

/// `KeyStore` structure, this contains a set of `KeyInfos` indexed by address.
#[derive(Clone, PartialEq, Debug, Eq)]
pub struct KeyStore {
    key_info: HashMap<String, KeyInfo>,
    persistence: Option<PersistentKeyStore>,
    encryption: Option<EncryptedKeyStore>,
}

pub enum KeyStoreConfig {
    Memory,
    Persistent(PathBuf),
    Encrypted(PathBuf, String),
}

/// Persistent `KeyStore` in JSON clear text in `KEYSTORE_LOCATION`
#[derive(Clone, PartialEq, Debug, Eq)]
struct PersistentKeyStore {
    file_path: PathBuf,
}

/// Encrypted `KeyStore`
/// `Argon2id` hash key derivation
/// `XSalsa20Poly1305` authenticated encryption
/// CBOR encoding
#[derive(Clone, PartialEq, Debug, Eq)]
struct EncryptedKeyStore {
    salt: SaltByteArray,
    encryption_key: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum EncryptedKeyStoreError {
    /// An error occurred while encrypting keys
    #[error("Error encrypting data")]
    EncryptionError,
}

impl KeyStore {
    pub fn new(config: KeyStoreConfig) -> Result<Self, Error> {
        match config {
            KeyStoreConfig::Memory => Ok(Self {
                key_info: HashMap::new(),
                persistence: None,
                encryption: None,
            }),
            KeyStoreConfig::Persistent(location) => {
                let file_path = location.join(KEYSTORE_NAME);

                match File::open(&file_path) {
                    Ok(file) => {
                        let reader = BufReader::new(file);

                        // Existing cleartext JSON keystore
                        let persisted_key_info: HashMap<String, PersistentKeyInfo> =
                            serde_json::from_reader(reader)
                                .inspect_err(|error| {
                                    error!(%error, "failed to deserialize keyfile, initializing new keystore at: {}.", file_path.display());
                                })
                                .unwrap_or_default();

                        let mut key_info = HashMap::new();
                        for (key, value) in persisted_key_info.iter() {
                            key_info.insert(
                                key.to_string(),
                                KeyInfo {
                                    private_key: BASE64_STANDARD
                                        .decode(value.private_key.clone())
                                        .map_err(|error| Error::Other(error.to_string()))?,
                                    key_type: value.key_type,
                                },
                            );
                        }

                        Ok(Self {
                            key_info,
                            persistence: Some(PersistentKeyStore { file_path }),
                            encryption: None,
                        })
                    }
                    Err(e) => {
                        if e.kind() == ErrorKind::NotFound {
                            warn!(
                                "Keystore does not exist, initializing new keystore at: {}",
                                file_path.display()
                            );
                            Ok(Self {
                                key_info: HashMap::new(),
                                persistence: Some(PersistentKeyStore { file_path }),
                                encryption: None,
                            })
                        } else {
                            Err(Error::Other(e.to_string()))
                        }
                    }
                }
            }
            KeyStoreConfig::Encrypted(location, passphrase) => {
                if !location.exists() {
                    create_dir(location.clone())?;
                }

                let file_path = location.join(Path::new(ENCRYPTED_KEYSTORE_NAME));

                if !file_path.exists() {
                    File::create(file_path.clone())?;
                }

                match File::open(&file_path) {
                    Ok(file) => {
                        let mut reader = BufReader::new(file);
                        let mut buf = vec![];
                        let read_bytes = reader.read_to_end(&mut buf)?;

                        if read_bytes == 0 {
                            // New encrypted keystore if file exists but is zero bytes (i.e., touch)
                            warn!(
                                "Keystore does not exist, initializing new keystore at {:?}",
                                file_path
                            );

                            let (salt, encryption_key) =
                                EncryptedKeyStore::derive_key(&passphrase, None).map_err(
                                    |error| {
                                        error!("Failed to create key from passphrase");
                                        Error::Other(error.to_string())
                                    },
                                )?;
                            Ok(Self {
                                key_info: HashMap::new(),
                                persistence: Some(PersistentKeyStore { file_path }),
                                encryption: Some(EncryptedKeyStore {
                                    salt,
                                    encryption_key,
                                }),
                            })
                        } else {
                            // Existing encrypted keystore
                            // Split off data from prepended salt
                            let data = buf.split_off(RECOMMENDED_SALT_LEN);
                            let mut prev_salt = [0; RECOMMENDED_SALT_LEN];
                            prev_salt.copy_from_slice(&buf);
                            let (salt, encryption_key) =
                                EncryptedKeyStore::derive_key(&passphrase, Some(prev_salt))
                                    .map_err(|error| {
                                        error!("Failed to create key from passphrase");
                                        Error::Other(error.to_string())
                                    })?;

                            let decrypted_data = EncryptedKeyStore::decrypt(&encryption_key, &data)
                                .map_err(|error| Error::Other(error.to_string()))?;

                            let key_info = from_slice_with_fallback(&decrypted_data)
                                .inspect_err(|error| {
                                    error!(%error, "Failed to deserialize keyfile, initializing new");
                                })
                                .unwrap_or_default();

                            Ok(Self {
                                key_info,
                                persistence: Some(PersistentKeyStore { file_path }),
                                encryption: Some(EncryptedKeyStore {
                                    salt,
                                    encryption_key,
                                }),
                            })
                        }
                    }
                    Err(_) => {
                        warn!("Encrypted keystore does not exist, initializing new keystore");

                        let (salt, encryption_key) =
                            EncryptedKeyStore::derive_key(&passphrase, None).map_err(|error| {
                                error!("Failed to create key from passphrase");
                                Error::Other(error.to_string())
                            })?;

                        Ok(Self {
                            key_info: HashMap::new(),
                            persistence: Some(PersistentKeyStore { file_path }),
                            encryption: Some(EncryptedKeyStore {
                                salt,
                                encryption_key,
                            }),
                        })
                    }
                }
            }
        }
    }

    pub fn flush(&self) -> anyhow::Result<()> {
        match &self.persistence {
            Some(persistent_keystore) => {
                let file = create_new_sensitive_file(&persistent_keystore.file_path)?;

                let mut writer = BufWriter::new(file);

                match &self.encryption {
                    Some(encrypted_keystore) => {
                        // Flush For EncryptedKeyStore
                        let data = serde_ipld_dagcbor::to_vec(&self.key_info).map_err(|e| {
                            Error::Other(format!("failed to serialize and write key info: {e}"))
                        })?;

                        let encrypted_data =
                            EncryptedKeyStore::encrypt(&encrypted_keystore.encryption_key, &data)?;
                        let mut salt_vec = encrypted_keystore.salt.to_vec();
                        salt_vec.extend(encrypted_data);
                        writer.write_all(&salt_vec)?;

                        Ok(())
                    }
                    None => {
                        let mut key_info: HashMap<String, PersistentKeyInfo> = HashMap::new();
                        for (key, value) in self.key_info.iter() {
                            key_info.insert(
                                key.to_string(),
                                PersistentKeyInfo {
                                    private_key: BASE64_STANDARD.encode(value.private_key.clone()),
                                    key_type: value.key_type,
                                },
                            );
                        }

                        // Flush for PersistentKeyStore
                        serde_json::to_writer_pretty(writer, &key_info).map_err(|e| {
                            Error::Other(format!("failed to serialize and write key info: {e}"))
                        })?;

                        Ok(())
                    }
                }
            }
            None => {
                // NoOp for MemKeyStore
                Ok(())
            }
        }
    }

    /// Return all of the keys that are stored in the `KeyStore`
    pub fn list(&self) -> Vec<String> {
        self.key_info.keys().cloned().collect()
    }

    /// Return `KeyInfo` that corresponds to a given key
    pub fn get(&self, k: &str) -> Result<KeyInfo, Error> {
        self.key_info.get(k).cloned().ok_or(Error::KeyInfo)
    }

    /// Save a key/`KeyInfo` pair to the `KeyStore`
    pub fn put(&mut self, key: &str, key_info: KeyInfo) -> Result<(), Error> {
        if self.key_info.contains_key(key) {
            return Err(Error::KeyExists);
        }
        self.key_info.insert(key.to_string(), key_info);

        if self.persistence.is_some() {
            self.flush().map_err(|err| Error::Other(err.to_string()))?;
        }

        Ok(())
    }

    /// Remove the key and corresponding `KeyInfo` from the `KeyStore`
    pub fn remove(&mut self, key: &str) -> anyhow::Result<KeyInfo> {
        let key_out = self.key_info.remove(key).ok_or(Error::KeyInfo)?;

        if self.persistence.is_some() {
            self.flush()?;
        }

        Ok(key_out)
    }
}

impl EncryptedKeyStore {
    fn derive_key(
        passphrase: &str,
        prev_salt: Option<SaltByteArray>,
    ) -> anyhow::Result<(SaltByteArray, Vec<u8>)> {
        let salt = match prev_salt {
            Some(prev_salt) => prev_salt,
            None => {
                let mut salt = [0; RECOMMENDED_SALT_LEN];
                crate::utils::rand::forest_os_rng().fill_bytes(&mut salt);
                salt
            }
        };

        let mut param_builder = ParamsBuilder::new();
        // #define crypto_pwhash_argon2id_MEMLIMIT_INTERACTIVE 67108864U
        // see <https://github.com/jedisct1/libsodium/blob/089f850608737f9d969157092988cb274fe7f8d4/src/libsodium/include/sodium/crypto_pwhash_argon2id.h#L70>
        const CRYPTO_PWHASH_ARGON2ID_MEMLIMIT_INTERACTIVE: u32 = 67108864;
        // #define crypto_pwhash_argon2id_OPSLIMIT_INTERACTIVE 2U
        // see <https://github.com/jedisct1/libsodium/blob/089f850608737f9d969157092988cb274fe7f8d4/src/libsodium/include/sodium/crypto_pwhash_argon2id.h#L66>
        const CRYPTO_PWHASH_ARGON2ID_OPSLIMIT_INTERACTIVE: u32 = 2;
        param_builder
            .m_cost(CRYPTO_PWHASH_ARGON2ID_MEMLIMIT_INTERACTIVE / 1024)
            .t_cost(CRYPTO_PWHASH_ARGON2ID_OPSLIMIT_INTERACTIVE);
        // https://docs.rs/sodiumoxide/latest/sodiumoxide/crypto/secretbox/xsalsa20poly1305/constant.KEYBYTES.html
        // KEYBYTES = 0x20
        // param_builder.output_len(32)?;
        let hasher = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            param_builder.build().map_err(map_err_to_anyhow)?,
        );
        let salt_string = SaltString::encode_b64(&salt).map_err(map_err_to_anyhow)?;
        let pw_hash = hasher
            .hash_password(passphrase.as_bytes(), &salt_string)
            .map_err(map_err_to_anyhow)?;
        if let Some(hash) = pw_hash.hash {
            Ok((salt, hash.as_bytes().to_vec()))
        } else {
            anyhow::bail!(EncryptedKeyStoreError::EncryptionError)
        }
    }

    fn encrypt(encryption_key: &[u8], msg: &[u8]) -> anyhow::Result<Vec<u8>> {
        let mut nonce = [0; NONCE_SIZE];
        crate::utils::rand::forest_os_rng().fill_bytes(&mut nonce);
        let nonce = GenericArray::from_slice(&nonce);
        let key = GenericArray::from_slice(encryption_key);
        let cipher = XSalsa20Poly1305::new(key);
        let mut ciphertext = cipher.encrypt(nonce, msg).map_err(map_err_to_anyhow)?;
        ciphertext.extend(nonce.iter());
        Ok(ciphertext)
    }

    #[allow(clippy::indexing_slicing)]
    fn decrypt(encryption_key: &[u8], msg: &[u8]) -> anyhow::Result<Vec<u8>> {
        anyhow::ensure!(msg.len() > NONCE_SIZE);
        let cyphertext_len = msg.len() - NONCE_SIZE;
        let ciphertext = &msg[..cyphertext_len];
        let nonce = GenericArray::from_slice(&msg[cyphertext_len..]);
        let key = GenericArray::from_slice(encryption_key);
        let cipher = XSalsa20Poly1305::new(key);
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(map_err_to_anyhow)?;
        Ok(plaintext)
    }
}

fn map_err_to_anyhow<T: Display>(e: T) -> anyhow::Error {
    anyhow::Error::msg(e.to_string())
}

#[cfg(test)]
mod test {
    use base64::{Engine, prelude::BASE64_STANDARD};

    use super::*;
    use crate::key_management::wallet;

    const PASSPHRASE: &str = "foobarbaz";

    #[test]
    fn test_generate_key() {
        let (salt, encryption_key) = EncryptedKeyStore::derive_key(PASSPHRASE, None).unwrap();
        let (second_salt, second_key) =
            EncryptedKeyStore::derive_key(PASSPHRASE, Some(salt)).unwrap();

        assert_eq!(
            encryption_key, second_key,
            "Derived key must be deterministic"
        );
        assert_eq!(salt, second_salt, "Salts must match");
    }

    #[test]
    fn test_encrypt_message() {
        let (_, private_key) = EncryptedKeyStore::derive_key(PASSPHRASE, None).unwrap();
        let message = "foo is coming";
        let ciphertext = EncryptedKeyStore::encrypt(&private_key, message.as_bytes()).unwrap();
        let second_pass = EncryptedKeyStore::encrypt(&private_key, message.as_bytes()).unwrap();
        assert_ne!(
            ciphertext, second_pass,
            "Ciphertexts use secure initialization vectors"
        );
    }

    #[test]
    fn test_decrypt_message() {
        let (_, private_key) = EncryptedKeyStore::derive_key(PASSPHRASE, None).unwrap();
        let message = "foo is coming";
        let ciphertext = EncryptedKeyStore::encrypt(&private_key, message.as_bytes()).unwrap();
        let plaintext = EncryptedKeyStore::decrypt(&private_key, &ciphertext).unwrap();
        assert_eq!(plaintext, message.as_bytes());
    }

    #[test]
    fn test_read_old_encrypted_keystore() {
        let dir: PathBuf = "src/key_management/tests/keystore_encrypted_old".into();
        assert!(dir.exists());
        let ks = KeyStore::new(KeyStoreConfig::Encrypted(dir, PASSPHRASE.to_string())).unwrap();
        assert!(ks.persistence.is_some());
    }

    #[test]
    fn test_read_write_encrypted_keystore() {
        let keystore_location = tempfile::tempdir().unwrap().keep();
        let ks = KeyStore::new(KeyStoreConfig::Encrypted(
            keystore_location.clone(),
            PASSPHRASE.to_string(),
        ))
        .unwrap();
        ks.flush().unwrap();

        let ks_read = KeyStore::new(KeyStoreConfig::Encrypted(
            keystore_location,
            PASSPHRASE.to_string(),
        ))
        .unwrap();

        assert_eq!(ks, ks_read);
    }

    #[test]
    fn test_read_write_keystore() {
        let keystore_location = tempfile::tempdir().unwrap().keep();
        let mut ks = KeyStore::new(KeyStoreConfig::Persistent(keystore_location.clone())).unwrap();

        let key = wallet::generate_key(SignatureType::Bls).unwrap();

        let addr = format!("wallet-{}", key.address);
        ks.put(&addr, key.key_info).unwrap();
        ks.flush().unwrap();

        let default = ks.get(&addr).unwrap();

        // Manually parse keystore.json
        let keystore_file = keystore_location.join(KEYSTORE_NAME);
        let reader = BufReader::new(File::open(keystore_file).unwrap());
        let persisted_keystore: HashMap<String, PersistentKeyInfo> =
            serde_json::from_reader(reader).unwrap();

        let default_key_info = persisted_keystore.get(&addr).unwrap();
        let actual = BASE64_STANDARD
            .decode(default_key_info.private_key.clone())
            .unwrap();

        assert_eq!(
            default.private_key, actual,
            "persisted key matches key from key store"
        );

        // Read existing keystore.json
        let ks_read = KeyStore::new(KeyStoreConfig::Persistent(keystore_location)).unwrap();
        assert_eq!(ks, ks_read);
    }
}
