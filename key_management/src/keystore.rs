// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use crypto::SignatureType;
use std::collections::HashMap;

/// KeyInfo struct, this contains the type of key (stored as a string) and the private key.
/// note how the private key is stored as a byte vector
#[derive(Clone, PartialEq, Debug, Eq)]
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

/// KeyStore struct, this contains a HashMap that is a set of KeyInfos resolved by their Address
pub trait KeyStore {
    /// Return all of the keys that are stored in the KeyStore
    fn list(&self) -> Vec<String>;
    /// Return Keyinfo that corresponds to a given key
    fn get(&self, k: &str) -> Result<KeyInfo, Error>;
    /// Save a key key_info pair to the KeyStore
    fn put(&mut self, key: String, key_info: KeyInfo) -> Result<(), Error>;
    /// Remove the Key and corresponding key_info from the KeyStore
    fn remove(&mut self, key: String) -> Option<KeyInfo>;
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

    fn remove(&mut self, key: String) -> Option<KeyInfo> {
        self.key_info.remove(&key)
    }
}
