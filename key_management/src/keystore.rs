// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

extern crate serde_json;

use super::errors::Error;
use crypto::SignatureType;
use log::{error, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, ErrorKind};
use std::path::Path;

const KEYSTORE_NAME: &str = "/keystore.json";

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

    /// Return a clone of the key_type
    pub fn key_type(&self) -> &SignatureType {
        &self.key_type
    }

    /// Return a clone of the private_key
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
pub trait KeyStore {
    /// Return all of the keys that are stored in the KeyStore
    fn list(&self) -> Vec<String>;
    /// Return Keyinfo that corresponds to a given key
    fn get(&self, k: &str) -> Result<KeyInfo, Error>;
    /// Save a key key_info pair to the KeyStore
    fn put(&mut self, key: String, key_info: KeyInfo) -> Result<(), Error>;
    /// Remove the Key and corresponding key_info from the KeyStore
    fn remove(&mut self, key: String) -> Result<KeyInfo, Error>;
}

#[derive(Default, Clone, PartialEq, Debug, Eq)]
pub struct MemKeyStore {
    pub key_info: HashMap<String, KeyInfo>,
}

impl MemKeyStore {
    /// Return a new empty KeyStore
    pub fn new() -> Self {
        MemKeyStore {
            key_info: HashMap::new(),
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
}

/// KeyStore that persists data in KEYSTORE_LOCATION
#[derive(Default, Clone, PartialEq, Debug, Eq)]
pub struct PersistentKeyStore {
    pub key_info: HashMap<String, KeyInfo>,
    location: String,
}

impl PersistentKeyStore {
    pub fn new(location: String) -> Result<Self, Error> {
        let loc = format!("{}{}", location, KEYSTORE_NAME);
        let file_op = File::open(&loc);
        match file_op {
            Ok(file) => {
                let reader = BufReader::new(file);
                let data: HashMap<String, KeyInfo> = serde_json::from_reader(reader)
                    .map_err(|e| {
                        error!("failed to deserialize keyfile, initializing new");
                        e
                    })
                    .unwrap_or_default();
                Ok(Self {
                    key_info: data,
                    location: loc,
                })
            }
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    warn!("keystore.json does not exist, initializing new keystore");
                    Ok(Self {
                        key_info: HashMap::new(),
                        location: loc,
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
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &self.key_info)
            .map_err(|e| Error::Other(format!("failed to serialize and write key info: {}", e)))?;
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
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&self.location)
            .map_err(|err| Error::Other(err.to_string()))?;
        serde_json::to_writer(&file, &self.key_info)
            .map_err(|err| Error::Other(err.to_string()))?;
        Ok(())
    }

    fn remove(&mut self, key: String) -> Result<KeyInfo, Error> {
        let key_out = self.key_info.remove(&key).ok_or(Error::KeyInfo)?;
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&self.location)
            .map_err(|err| Error::Other(err.to_string()))?;
        serde_json::to_writer(file, &self.key_info).map_err(|err| Error::Other(err.to_string()))?;
        Ok(key_out)
    }
}
