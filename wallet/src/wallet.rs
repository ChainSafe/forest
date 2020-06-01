// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use super::{KeyInfo, KeyStore};
use address::Address;
use crypto::{Signature, SignatureType};
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Clone, PartialEq, Debug, Eq)]
pub struct Key {
    key_info: KeyInfo,
    public_key: Vec<u8>,
    address: Address,
}

impl Key {
    pub fn new(key_info: &KeyInfo) -> Self {
        unimplemented!()
    }
}

#[derive(Clone, PartialEq, Debug, Eq)]
pub struct Wallet {
    keys: HashMap<Address, Key>,
    keystore: KeyStore,
}

impl Wallet {
    pub fn new(keystore: KeyStore) -> Self {
        Wallet {
            keys: HashMap::new(),
            keystore,
        }
    }

    pub fn new_from_keys(key_vec: Vec<Key>) -> Self {
        let mut keys: HashMap<Address, Key> = HashMap::new();
        for item in key_vec.clone() {
            keys.insert(item.address, item);
        }
        Wallet {
            keys,
            keystore: KeyStore::new(),
        }
    }

    pub fn find_key(&mut self, addr: &Address) -> Result<Key, Error> {
        let key = self.keys.get(&addr);
        if let Some(k) = key {
            return Ok(k.clone());
        }
        let mut owned_string = "wallet-".to_owned();
        owned_string.push_str(addr.to_string().as_ref());
        let key_info = self.keystore.get(&owned_string)?;
        let new_key = Key::new(key_info);
        self.keys.insert(addr.clone(), new_key.clone());
        Ok(new_key)
    }

    /// TODO will need to implement this after more research about signing messages is done
    pub fn sign(&mut self, addr: &Address, msg: Vec<u8>) -> Result<Signature, Error> {
        unimplemented!()
    }

    pub fn export(&mut self, addr: &Address) -> Result<KeyInfo, Error> {
        let k = self.find_key(addr)?;
        Ok(k.key_info)
    }

    pub fn import(&mut self, key_info: &KeyInfo) -> Result<Address, Error> {
        let k = Key::new(key_info);
        let mut owned_string = "wallet-".to_owned();
        owned_string.push_str(k.address.to_string().as_ref());
        self.keystore.put(owned_string, k.key_info)?;
        Ok(k.address)
    }

    pub fn list_addrs(&self) -> Result<Vec<Address>, Error> {
        let mut all = self.keystore.list();
        all.sort();
        let mut out = Vec::new();
        for i in all {
            if i.starts_with("wallet-") {
                // TODO replace this with strip_prefix after it has been added to stable rust
                let name = i.trim_start_matches("wallet-");
                let addr = Address::from_str(name).map_err(|err| Error::Other(err.to_string()))?;
                out.push(addr);
            }
        }
        Ok(out)
    }

    pub fn get_default(&self) -> Result<Address, Error> {
        let key_info = self.keystore.get(&"default".to_string())?;
        let k = Key::new(key_info);
        Ok(k.address)
    }

    pub fn set_default(&mut self, addr: Address) -> Result<(), Error> {
        let mut owned_string = "wallet-".to_owned();
        owned_string.push_str(addr.to_string().as_ref());
        let key_info = self.keystore.get(&owned_string)?.clone();
        // TODO change this code to not exit if there is no kv pair with default key in keystore
        self.keystore.remove("wallet-".to_string())?;
        self.keystore.put("wallet-".to_string(), key_info)?;
        Ok(())
    }

    pub fn generate_key(&mut self, typ: SignatureType) -> Result<Address, Error> {
        let key = generate_key(typ)?;
        let mut owned_string = "wallet-".to_owned();
        owned_string.push_str(key.address.to_string().as_ref());
        self.keystore.put(owned_string, key.key_info.clone())?;
        self.keys.insert(key.address, key.clone());
        let value = self.keystore.get(&"default".to_string());
        if let Err(_) = value {
            self
                .keystore
                .put("default".to_string(), key.key_info.clone())
                .map_err(|err| Error::Other(err.to_string()))?;
        }

        Ok(key.address)
    }

    pub fn has_key(&mut self, addr: &Address) -> bool {
        self.find_key(addr).map_or_else(|_| false, |_| true)

    }
}

pub fn kstore_sig_type(typ: SignatureType) -> String {
    match typ {
        SignatureType::Secp256 => "secp256k1".to_string(),
        _ => "bls".to_string(),
    }
}

pub fn act_sig_type(typ: String) -> SignatureType {
    if typ == "secp256k1".to_string() {
        return SignatureType::Secp256;
    }
    SignatureType::default()
}

/// TODO need to complete when generating a private key for a given type is implemented, see lotus
fn generate_key(typ: SignatureType) -> Result<Key, Error> {
    // let public_key = Signature::gen
    unimplemented!()
}
