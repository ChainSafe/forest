// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use std::collections::HashMap;

/// KeyInfo struct, this contains the type of key (stored as a string) and the private key.
/// note how the private key is stored as a byte vector
#[derive(Clone, PartialEq, Debug, Eq)]
pub struct KeyInfo {
    key_type: String,
    // sadly Vec<u8> will be used because Eq has not been implemented for PrivateKey type
    private_key: Vec<u8>,
}

impl KeyInfo {
    /// Return a new KeyInfo given the key_type and private_key
    pub fn new(key_type: String, private_key: Vec<u8>) -> Self {
        KeyInfo {
            key_type,
            private_key,
        }
    }

    /// Return a clone of the key_type
    pub fn key_type(&self) -> String {
        self.key_type.clone()
    }

    /// Return a clone of the private_key
    pub fn private_key(&self) -> Vec<u8> {
        self.private_key.clone()
    }
}

/// KeyStore struct, this contains a HashMap that is a set of KeyInfos resolved by their Address
#[derive(Default, Clone, PartialEq, Debug, Eq)]
pub struct KeyStore {
    pub key_inf: HashMap<String, KeyInfo>,
}

impl KeyStore {
    /// Return a new empty KeyStore
    pub fn new() -> Self {
        KeyStore { key_inf: HashMap::new() }
    }

    /// Return all of the keys that are stored in the KeyStore
    pub fn list(&self) -> Vec<String> {
        self.key_inf.iter().map(|(key, _ )| key.clone()).collect()
    }

    /// Return Keyinfo that corresponds to a given key
    pub fn get(&self, k: &str) -> Result<&KeyInfo, Error> {
        self.key_inf.get(k).ok_or(Error::KeyInfo)
    }

    /// Save a key key_info pair to the KeyStore
    pub fn put(&mut self, key: String, key_info: KeyInfo) -> Result<(), Error> {
        if self.key_inf.contains_key(&key) {
            return Err(Error::KeyExists);
        }
        self.key_inf.insert(key, key_info);
        Ok(())
    }

    /// Remove the Key and corresponding key_info from the KeyStore
    pub fn remove(&mut self, key: String) -> Option<KeyInfo> {
        self.key_inf.remove(&key)
    }
}
