// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use log::{error, warn};
use serde::{Deserialize, Serialize};
use sodiumoxide::crypto::pwhash::argon2id13 as pwhash;
use sodiumoxide::crypto::secretbox;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

use super::errors::Error;
use crypto::SignatureType;

pub const KEYSTORE_NAME: &str = "keystore.json";
pub const ENCRYPTED_KEYSTORE_NAME: &str = "keystore";

/// KeyInfo struct, this contains the type of key (stored as a string) and the private key.
/// note how the private key is stored as a byte vector
///
/// TODO need to update keyinfo to not use SignatureType, use string instead to save keys like
/// jwt secret
#[derive(Clone, PartialEq, Debug, Eq, Serialize, Deserialize)]
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
    /// Return a new KeyInfo given the key_type and private_key
    pub fn new(key_type: SignatureType, private_key: Vec<u8>) -> Self {
        KeyInfo {
            key_type,
            private_key,
        }
    }

    /// Return a reference to the key_type
    pub fn key_type(&self) -> &SignatureType {
        &self.key_type
    }

    /// Return a reference to the private_key
    pub fn private_key(&self) -> &Vec<u8> {
        &self.private_key
    }
}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use crypto::signature::json::signature_type::SignatureTypeJson;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and deserializing a SignedMessage from JSON.
    #[derive(Clone, Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct KeyInfoJson(#[serde(with = "self")] pub KeyInfo);

    /// Wrapper for serializing a SignedMessage reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct KeyInfoJsonRef<'a>(#[serde(with = "self")] pub &'a KeyInfo);

    impl From<KeyInfoJson> for KeyInfo {
        fn from(key: KeyInfoJson) -> KeyInfo {
            key.0
        }
    }
    #[derive(Serialize, Deserialize)]
    struct JsonHelper {
        #[serde(rename = "Type")]
        sig_type: SignatureTypeJson,
        #[serde(rename = "PrivateKey")]
        private_key: String,
    }

    pub fn serialize<S>(k: &KeyInfo, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            sig_type: SignatureTypeJson(k.key_type),
            private_key: base64::encode(&k.private_key),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<KeyInfo, D::Error>
    where
        D: Deserializer<'de>,
    {
        let JsonHelper {
            sig_type,
            private_key,
        } = Deserialize::deserialize(deserializer)?;
        Ok(KeyInfo {
            key_type: sig_type.0,
            private_key: base64::decode(private_key).map_err(de::Error::custom)?,
        })
    }
}

/// KeyStore struct, this contains a HashMap that is a set of KeyInfos resolved by their Address
pub trait Store {
    /// Return all of the keys that are stored in the KeyStore
    fn list(&self) -> Vec<String>;
    /// Return Keyinfo that corresponds to a given key
    fn get(&self, k: &str) -> Result<KeyInfo, Error>;
    /// Save a key key_info pair to the KeyStore
    fn put(&mut self, key: String, key_info: KeyInfo) -> Result<(), Error>;
    /// Remove the Key and corresponding key_info from the KeyStore
    fn remove(&mut self, key: String) -> Result<KeyInfo, Error>;
}

/// KeyStore struct, this contains a HashMap that is a set of KeyInfos resolved by their Address
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

/// Persistent KeyStore in JSON cleartext in KEYSTORE_LOCATION
#[derive(Clone, PartialEq, Debug, Eq)]
struct PersistentKeyStore {
    file_path: PathBuf,
}

/// Encrypted KeyStore
/// Argon2id hash key derivation
/// XSalsa20Poly1305 authenticated encryption
/// CBOR encoding
#[derive(Clone, PartialEq, Debug, Eq)]
struct EncryptedKeyStore {
    salt: pwhash::Salt,
    encryption_key: Arc<secretbox::Key>,
}

#[derive(Debug, Error)]
pub enum EncryptedKeyStoreError {
    /// Possibly indicates incorrect passphrase
    #[error("Error decrypting data")]
    DecryptionError,
    /// An error occured while encrypting keys
    #[error("Error encrypting data")]
    EncryptionError,
    /// Unlock called without `encrypted_keystore` being enabled in config.toml
    #[error("Error with forest configuration")]
    ConfigurationError,
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
                                .map_err(|e| {
                                    error!(
                                "failed to deserialize keyfile, initializing new keystore at: {:?}",
                                file_path
                            );
                                    e
                                })
                                .unwrap_or_default();

                        let mut key_info = HashMap::new();
                        for (key, value) in persisted_key_info.iter() {
                            key_info.insert(
                                key.to_string(),
                                KeyInfo {
                                    private_key: base64::decode(value.private_key.clone())
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
                                "Keystore does not exist, initializing new keystore at: {:?}",
                                file_path
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
                let file_path = location.join(Path::new(ENCRYPTED_KEYSTORE_NAME));

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
                            let data = buf.split_off(pwhash::SALTBYTES);

                            let (salt, encryption_key) =
                                EncryptedKeyStore::derive_key(&passphrase, Some(buf)).map_err(
                                    |error| {
                                        error!("Failed to create key from passphrase");
                                        Error::Other(error.to_string())
                                    },
                                )?;

                            let decrypted_data =
                                EncryptedKeyStore::decrypt(encryption_key.clone(), &data)
                                    .map_err(|error| Error::Other(error.to_string()))?;

                            let key_info = serde_cbor::from_slice(&decrypted_data)
                                .map_err(|e| {
                                    error!("Failed to deserialize keyfile, initializing new");
                                    e
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

    pub fn flush(&self) -> Result<(), Error> {
        match &self.persistence {
            Some(persistent_keystore) => {
                let dir = persistent_keystore
                    .file_path
                    .parent()
                    .ok_or_else(|| Error::Other("Invalid Path".to_string()))?;
                fs::create_dir_all(dir)?;
                let file = File::create(&persistent_keystore.file_path)?;

                // Restrict permissions on files containing private keys
                #[cfg(unix)]
                utils::set_user_perm(&file)?;

                let mut writer = BufWriter::new(file);

                match &self.encryption {
                    Some(encrypted_keystore) => {
                        // Flush For EncryptedKeyStore
                        let data = serde_cbor::to_vec(&self.key_info).map_err(|e| {
                            Error::Other(format!("failed to serialize and write key info: {}", e))
                        })?;

                        let encrypted_data = EncryptedKeyStore::encrypt(
                            encrypted_keystore.encryption_key.clone(),
                            &data,
                        );

                        let mut salt_vec = encrypted_keystore.salt.as_ref().to_vec();
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
                                    private_key: base64::encode(value.private_key.clone()),
                                    key_type: value.key_type,
                                },
                            );
                        }

                        // Flush for PersistentKeyStore
                        serde_json::to_writer_pretty(writer, &key_info).map_err(|e| {
                            Error::Other(format!("failed to serialize and write key info: {}", e))
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

    /// Return all of the keys that are stored in the KeyStore
    pub fn list(&self) -> Vec<String> {
        self.key_info.iter().map(|(key, _)| key.clone()).collect()
    }

    /// Return Keyinfo that corresponds to a given key
    pub fn get(&self, k: &str) -> Result<KeyInfo, Error> {
        self.key_info.get(k).cloned().ok_or(Error::KeyInfo)
    }

    /// Save a key key_info pair to the KeyStore
    pub fn put(&mut self, key: String, key_info: KeyInfo) -> Result<(), Error> {
        if self.key_info.contains_key(&key) {
            return Err(Error::KeyExists);
        }
        self.key_info.insert(key, key_info);

        if self.persistence.is_some() {
            self.flush()?;
        }

        Ok(())
    }

    /// Remove the Key and corresponding key_info from the KeyStore
    pub fn remove(&mut self, key: String) -> Result<KeyInfo, Error> {
        let key_out = self.key_info.remove(&key).ok_or(Error::KeyInfo)?;

        if self.persistence.is_some() {
            self.flush()?;
        }

        Ok(key_out)
    }
}

impl EncryptedKeyStore {
    fn derive_key(
        passphrase: &str,
        prev_salt: Option<Vec<u8>>,
    ) -> Result<(pwhash::Salt, Arc<secretbox::Key>), EncryptedKeyStoreError> {
        let salt = match prev_salt {
            Some(prev_salt) => match pwhash::Salt::from_slice(&prev_salt) {
                Some(salt) => salt,
                None => return Err(EncryptedKeyStoreError::ConfigurationError),
            },
            None => pwhash::gen_salt(),
        };
        let mut key = secretbox::Key([0; secretbox::KEYBYTES]);

        let secretbox::Key(ref mut kb) = key;
        pwhash::derive_key(
            kb,
            passphrase.as_bytes(),
            &salt,
            pwhash::OPSLIMIT_INTERACTIVE,
            pwhash::MEMLIMIT_INTERACTIVE,
        )
        .unwrap();

        Ok((salt, Arc::new(key)))
    }

    fn encrypt(encryption_key: Arc<secretbox::Key>, msg: &[u8]) -> Vec<u8> {
        let nonce = secretbox::gen_nonce();

        let mut ciphertext = secretbox::seal(msg, &nonce, &encryption_key);
        ciphertext.append(&mut nonce.as_ref().to_vec());
        ciphertext
    }

    fn decrypt(
        encryption_key: Arc<secretbox::Key>,
        msg: &[u8],
    ) -> Result<Vec<u8>, EncryptedKeyStoreError> {
        let ciphertext = &msg[..msg.len() - 24];

        let nonce = secretbox::Nonce::from_slice(&msg[msg.len() - 24..])
            .ok_or(EncryptedKeyStoreError::DecryptionError)?;

        let plaintext = secretbox::open(ciphertext, &nonce, &encryption_key)
            .map_err(|_| EncryptedKeyStoreError::DecryptionError)?;

        Ok(plaintext)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::wallet;

    const PASSPHRASE: &'static str = "foobarbaz";

    #[test]
    fn test_generate_key() {
        let (salt, encryption_key) = EncryptedKeyStore::derive_key(PASSPHRASE, None).unwrap();
        let (second_salt, second_key) =
            EncryptedKeyStore::derive_key(PASSPHRASE, Some(salt.as_ref().to_vec())).unwrap();

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
        let ciphertext = EncryptedKeyStore::encrypt(private_key.clone(), message.as_bytes());
        let second_pass = EncryptedKeyStore::encrypt(private_key.clone(), message.as_bytes());

        assert_ne!(
            ciphertext, second_pass,
            "Ciphertexts use secure initialization vectors"
        );
    }

    #[test]
    fn test_decrypt_message() {
        let (_, private_key) = EncryptedKeyStore::derive_key(PASSPHRASE, None).unwrap();
        let message = "foo is coming";
        let ciphertext = EncryptedKeyStore::encrypt(private_key.clone(), message.as_bytes());
        let plaintext = EncryptedKeyStore::decrypt(private_key.clone(), &ciphertext).unwrap();

        assert_eq!(plaintext, message.as_bytes());
    }

    #[test]
    #[ignore = "fragile test, requires encrypted keystore to exist"]
    fn test_read_encrypted_keystore() {
        // todo: change this to read config.toml
        // this test requires an encrypted keystore
        // current way to run this test:
        // add encrypt_keystore = true to config.toml
        // change keystore_location to your location
        let keystore_location = PathBuf::from("/tmp/forest-db");
        let ks = KeyStore::new(KeyStoreConfig::Encrypted(
            keystore_location,
            PASSPHRASE.to_string(),
        ))
        .unwrap();
        ks.flush().unwrap();

        assert!(true);
    }

    #[test]
    #[ignore = "fragile test, requires keystore.json to exist"]
    fn test_encode_read_write() {
        let keystore_location = PathBuf::from("/tmp/forest-db");
        let mut ks = KeyStore::new(KeyStoreConfig::Persistent(keystore_location.clone())).unwrap();

        let key = wallet::generate_key(SignatureType::BLS).unwrap();

        let addr = format!("wallet-{}", key.address.to_string());
        ks.put(addr.clone(), key.key_info.clone()).unwrap();
        ks.flush().unwrap();

        let default = ks.get(&addr).unwrap();

        let mut keystore_file = keystore_location.clone();
        keystore_file.push("keystore.json");

        let reader = BufReader::new(File::open(keystore_file).unwrap());
        let persisted_keystore: HashMap<String, PersistentKeyInfo> =
            serde_json::from_reader(reader).unwrap();

        let default_key_info = persisted_keystore.get(&addr).unwrap();
        let actual = base64::decode(default_key_info.private_key.clone()).unwrap();

        assert_eq!(
            default.private_key, actual,
            "persisted key matches key from key store"
        );
    }

    #[test]
    #[ignore = "fragile test, requires keystore.json"]
    fn test_read_unencrypted_keystore() {
        let keystore_location = PathBuf::from("/tmp/forest-db");
        let ks = KeyStore::new(KeyStoreConfig::Persistent(keystore_location)).unwrap();
        ks.flush().unwrap();

        assert!(true);
    }
}
