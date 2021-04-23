// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

extern crate serde_json;

use super::errors::Error;
use crypto::SignatureType;
use log::{error, warn};
use ring::{digest, pbkdf2};
use serde::{Deserialize, Serialize};
use sodiumoxide::crypto::secretbox;
use std::io::{BufReader, BufWriter, ErrorKind, Read, Write};
use std::path::Path;
use std::{collections::HashMap, num::NonZeroU32};
use std::{
    fs::{self, File},
    os::unix::prelude::OsStrExt,
};
use thiserror::Error;

const KEYSTORE_NAME: &str = "/keystore.json";
const ENCRYPTED_KEYSTORE_NAME: &str = "/keystore";
const GENERATED_KEY_LEN: usize = digest::SHA256_OUTPUT_LEN;
type GeneratedKey = [u8; GENERATED_KEY_LEN];
static PBKDF2_ALG: pbkdf2::Algorithm = pbkdf2::PBKDF2_HMAC_SHA256;

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

/// KeyStore struct, this contains a HashMap that is a set of KeyInfos resolved by their Address
pub trait KeyStore {
    /// Return all of the keys that are stored in the KeyStore
    fn list(&self) -> Vec<String>;
    /// Return Keyinfo that corresponds to a given key
    fn get(&self, k: &str) -> Result<KeyInfo, Error>;
    /// Save a key key_info pair to the KeyStore
    fn put(&mut self, key: String, key_info: KeyInfo) -> Result<(), Error>;
    /// Remove the Key and corresponding key_info from the KeyStore
    fn remove(&mut self, key: String) -> Result<KeyInfo, Error>;
    /// Derive a key from passphrase
    fn unlock(&mut self, passphrase: &str) -> Result<(), Error>;
}

pub trait EncryptedKeyStore {
    /// Generate a private key from a passphrase for encryption
    fn derive_key(passphrase: &str) -> Result<Vec<u8>, EncryptedKeyStoreError>;
    /// Encrypt a message using a symmetric key
    fn encrypt(key: &[u8], msg: &[u8]) -> Result<Vec<u8>, EncryptedKeyStoreError>;
    /// Decrypt a message using a symmetric key
    fn decrypt(key: &[u8], msg: &[u8]) -> Result<Vec<u8>, EncryptedKeyStoreError>;
}

#[derive(Default, Clone, PartialEq, Debug, Eq)]
pub struct MemKeyStore {
    pub key_info: HashMap<String, KeyInfo>,
    is_encrypted: bool,
    key: Option<Vec<u8>>,
}

impl MemKeyStore {
    /// Return a new empty KeyStore
    pub fn new() -> Self {
        MemKeyStore {
            key_info: HashMap::new(),
            is_encrypted: false,
            key: None,
        }
    }
}

impl KeyStore for MemKeyStore {
    fn list(&self) -> Vec<String> {
        self.key_info.iter().map(|(key, _)| key.clone()).collect()
    }

    fn get(&self, k: &str) -> Result<KeyInfo, Error> {
        self.key_info.get(k).cloned().ok_or(Error::KeyInfo)
    }

    fn put(&mut self, key: String, key_info: KeyInfo) -> Result<(), Error> {
        if self.key_info.contains_key(&key) {
            return Err(Error::KeyExists);
        }
        self.key_info.insert(key, key_info);
        Ok(())
    }

    fn remove(&mut self, key: String) -> Result<KeyInfo, Error> {
        self.key_info.remove(&key).ok_or(Error::KeyInfo)
    }

    fn unlock(&mut self, passphrase: &str) -> Result<(), Error> {
        let key = PersistentKeyStore::derive_key(passphrase)
            .map_err(|error| Error::Other(error.to_string()))?;
        self.key = Some(key);
        Ok(())
    }
}

/// KeyStore that persists data in KEYSTORE_LOCATION
#[derive(Default, Clone, PartialEq, Debug, Eq)]
pub struct PersistentKeyStore {
    pub key_info: HashMap<String, KeyInfo>,
    location: String,
    is_encrypted: bool,
    key: Option<Vec<u8>>,
}

impl PersistentKeyStore {
    pub fn new(
        location: String,
        encrypt_keystore: bool,
        passphrase: Option<String>,
    ) -> Result<Self, Error> {
        let loc = if encrypt_keystore {
            format!("{}{}", location, ENCRYPTED_KEYSTORE_NAME)
        } else {
            format!("{}{}", location, KEYSTORE_NAME)
        };
        let key = match passphrase {
            Some(value) => Some(PersistentKeyStore::derive_key(&value).map_err(|error| {
                error!("failed to create key from passphrase");
                Error::Other(error.to_string())
            })?),
            None => None,
        };

        let file_op = File::open(&loc);
        match file_op {
            Ok(file) => {
                let mut reader = BufReader::new(file);

                let mut buf = Vec::new();
                let data = if encrypt_keystore {
                    let read_bytes = reader.read_to_end(&mut buf)?;

                    if read_bytes <= 0 {
                        // store is new
                        return Ok(Self {
                            key_info: HashMap::new(),
                            location: loc,
                            is_encrypted: encrypt_keystore,
                            key,
                        });
                    }

                    let key = match &key {
                        Some(value) => value,
                        None => return Err(Error::Other("this shouldn't happen".to_string())),
                    };

                    let decrypted_data = PersistentKeyStore::decrypt(&key, &buf)
                        .map_err(|error| Error::Other(error.to_string()))?;

                    serde_cbor::from_slice(&decrypted_data)
                        .map_err(|e| {
                            error!("failed to deserialize keyfile, initializing new");
                            e
                        })
                        .unwrap_or_default()
                } else {
                    serde_json::from_reader(reader)
                        .map_err(|e| {
                            error!("failed to deserialize keyfile, initializing new");
                            e
                        })
                        .unwrap_or_default()
                };
                Ok(Self {
                    key_info: data,
                    location: loc,
                    is_encrypted: encrypt_keystore,
                    key,
                })
            }
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    warn!("keystore does not exist, initializing new keystore");
                    Ok(Self {
                        key_info: HashMap::new(),
                        location: loc,
                        is_encrypted: encrypt_keystore,
                        key,
                    })
                } else {
                    Err(Error::Other(e.to_string()))
                }
            }
        }
    }

    pub fn flush(&self) -> Result<(), Error> {
        let dir = Path::new(&self.location)
            .parent()
            .ok_or_else(|| Error::Other("Invalid Path".to_string()))?;
        fs::create_dir_all(dir)?;
        let file = File::create(&self.location)?;
        let mut writer = BufWriter::new(file);
        if self.is_encrypted {
            let data = serde_cbor::to_vec(&self.key_info).map_err(|e| {
                Error::Other(format!("failed to serialize and write key info: {}", e))
            })?;

            let key = match &self.key {
                Some(key) => key,
                None => return Err(Error::Other("Keystore is not unlocked".to_string())),
            };

            let encrypted_data = PersistentKeyStore::encrypt(&key, &data)
                .map_err(|error| Error::Other(error.to_string()))?;

            writer.write_all(&encrypted_data)?;
        } else {
            serde_json::to_writer(writer, &self.key_info).map_err(|e| {
                Error::Other(format!("failed to serialize and write key info: {}", e))
            })?;
        }
        Ok(())
    }
}

impl KeyStore for PersistentKeyStore {
    fn list(&self) -> Vec<String> {
        self.key_info.iter().map(|(key, _)| key.clone()).collect()
    }

    fn get(&self, k: &str) -> Result<KeyInfo, Error> {
        self.key_info.get(k).cloned().ok_or(Error::KeyInfo)
    }

    fn put(&mut self, key: String, key_info: KeyInfo) -> Result<(), Error> {
        if self.key_info.contains_key(&key) {
            return Err(Error::KeyExists);
        }

        self.key_info.insert(key, key_info);
        self.flush()?;
        Ok(())
    }

    fn remove(&mut self, key: String) -> Result<KeyInfo, Error> {
        let key_out = self.key_info.remove(&key).ok_or(Error::KeyInfo)?;
        self.flush()?;
        Ok(key_out)
    }

    fn unlock(&mut self, passphrase: &str) -> Result<(), Error> {
        let key = PersistentKeyStore::derive_key(passphrase)
            .map_err(|error| Error::Other(error.to_string()))?;
        self.key = Some(key);
        Ok(())
    }
}

impl EncryptedKeyStore for PersistentKeyStore {
    fn derive_key(passphrase: &str) -> Result<Vec<u8>, EncryptedKeyStoreError> {
        let hostname = hostname::get().map_err(|_| EncryptedKeyStoreError::ConfigurationError)?;

        let mut to_store: GeneratedKey = [0u8; GENERATED_KEY_LEN];

        pbkdf2::derive(
            PBKDF2_ALG,
            NonZeroU32::new(5).unwrap(),
            hostname.as_bytes(),
            passphrase.as_bytes(),
            &mut to_store,
        );

        Ok(to_store.to_vec())
    }

    fn encrypt(key: &[u8], msg: &[u8]) -> Result<Vec<u8>, EncryptedKeyStoreError> {
        let nonce = secretbox::gen_nonce();

        let key = match secretbox::Key::from_slice(key) {
            Some(value) => value,
            None => return Err(EncryptedKeyStoreError::EncryptionError),
        };

        let mut ciphertext = secretbox::seal(msg, &nonce, &key);
        ciphertext.append(&mut nonce.as_ref().to_vec());
        Ok(ciphertext)
    }

    fn decrypt(key: &[u8], msg: &[u8]) -> Result<Vec<u8>, EncryptedKeyStoreError> {
        let ciphertext = &msg[..msg.len() - 24];

        let nonce = match secretbox::Nonce::from_slice(&msg[msg.len() - 24..]) {
            Some(value) => value,
            None => return Err(EncryptedKeyStoreError::DecryptionError),
        };

        let key = match secretbox::Key::from_slice(&key) {
            Some(value) => value,
            None => return Err(EncryptedKeyStoreError::DecryptionError),
        };

        let plaintext = secretbox::open(&ciphertext, &nonce, &key)
            .map_err(|_| EncryptedKeyStoreError::DecryptionError)?;

        Ok(plaintext)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const PASSPHRASE: &'static str = "foobarbaz";

    #[test]
    fn test_generate_key() {
        let private_key = PersistentKeyStore::derive_key(PASSPHRASE).unwrap();
        let second_pass = PersistentKeyStore::derive_key(PASSPHRASE).unwrap();
        assert_eq!(private_key, second_pass);
    }

    #[test]
    fn test_encrypt_message() {
        let private_key = PersistentKeyStore::derive_key(PASSPHRASE).unwrap();
        let message = "foo is coming";
        let ciphertext = PersistentKeyStore::encrypt(&private_key, message.as_bytes());
        assert!(ciphertext.is_ok());
    }

    #[test]
    fn test_decrypt_message() {
        let private_key = PersistentKeyStore::derive_key(PASSPHRASE).unwrap();
        let message = "foo is coming";
        let ciphertext = PersistentKeyStore::encrypt(&private_key, message.as_bytes()).unwrap();
        let plaintext = PersistentKeyStore::decrypt(&private_key, &ciphertext).unwrap();

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
        let keystore_location = String::from("/home/connor/chainsafe/forest-db");
        let ks = PersistentKeyStore::new(keystore_location, true, Some(String::from(PASSPHRASE)))
            .unwrap();
        ks.flush().unwrap();

        assert!(true);
    }

    #[test]
    #[ignore = "fragile test, requires keystore.json"]
    fn test_read_unencrypted_keystore() {
        let keystore_location = String::from("/home/connor/chainsafe/forest-db");
        let ks = PersistentKeyStore::new(keystore_location, false, None).unwrap();
        ks.flush().unwrap();

        assert!(true);
    }
}
