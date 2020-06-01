// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use std::collections::HashMap;

#[derive(Clone, PartialEq, Debug, Eq)]
pub struct KeyInfo {
    key_type: String,
    private_key: Vec<u8>,
}

impl KeyInfo {
    pub fn key_type(&self) -> String {
        self.key_type.clone()
    }

    pub fn private_key(&self) -> Vec<u8> {
        self.private_key.clone()
    }
}

#[derive(Clone, PartialEq, Debug, Eq)]
pub struct KeyStore {
    pub m: HashMap<String, KeyInfo>,
}

impl KeyStore {
    pub fn new() -> Self {
        KeyStore { m: HashMap::new() }
    }

    /// Return all of the keys that are stored in the KeyStore
    pub fn list(&self) -> Vec<String> {
        let mut out_vec = Vec::new();

        for (key, _) in self.m.iter() {
            out_vec.push(key.clone())
        }
        out_vec
    }

    /// Return Keyinfo that corresponds to a given key
    pub fn get(&self, k: &String) -> Result<&KeyInfo, Error> {
        self.m.get(k).map_or_else(|| Err(Error::KeyInfo), |v| Ok(v))
    }

    /// Save a key key_info pair to the KeyStore
    pub fn put(&mut self, key: String, key_info: KeyInfo) -> Result<(), Error> {
        if self.m.contains_key(&key) {
            return Err(Error::KeyExists);
        }
        self.m.insert(key, key_info);
        Ok(())
    }

    pub fn remove(&mut self, key: String) -> Result<(), Error> {
        match self.m.remove(&key) {
            Some(_t) => Ok(()),
            None => Err(Error::NoKey),
        }
    }
}
