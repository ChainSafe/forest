// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use super::{wallet_helpers, KeyInfo, KeyStore};
use crate::MemKeyStore;
use address::Address;
use crypto::{Signature, SignatureType};
use std::collections::HashMap;
use std::str::FromStr;

/// A Key, this contains a key_info, address, and public_key which holds the key type and private key
#[derive(Clone, PartialEq, Debug, Eq)]
pub struct Key {
    key_info: KeyInfo,
    // Vec<u8> is used because The public keys for BLS and SECP256K1 are not of the same type
    public_key: Vec<u8>,
    address: Address,
}

impl From<KeyInfo> for Key {
    fn from(key_info: KeyInfo) -> Self {
        let public_key =
            wallet_helpers::to_public(*key_info.key_type(), key_info.private_key()).unwrap();
        let address = wallet_helpers::new_address(*key_info.key_type(), &public_key).unwrap();
        Key {
            key_info,
            public_key,
            address,
        }
    }
}

/// This is a Wallet, it contains 2 HashMaps:
/// - keys which is a HashMap of Keys resolved by their Address
/// - keystore which is a HashMap of KeyInfos resolved by their Address
#[derive(Clone, PartialEq, Debug, Eq)]
pub struct Wallet<T> {
    keys: HashMap<Address, Key>,
    keystore: T,
}

impl Wallet<MemKeyStore> {
    /// Return a wallet from a given amount of keys. This wallet will not use the
    /// generic keystore trait, but rather specifically use a MemKeyStore
    pub fn new_from_keys(key_vec: impl IntoIterator<Item = Key>) -> Self {
        let mut keys: HashMap<Address, Key> = HashMap::new();
        for item in key_vec.into_iter() {
            keys.insert(item.address, item);
        }
        let key_store = MemKeyStore::new();
        Wallet {
            keys,
            keystore: key_store,
        }
    }
}

impl<T> Wallet<T>
where
    T: KeyStore,
{
    /// Return a new Wallet with a given KeyStore
    pub fn new(keystore: T) -> Self {
        Wallet {
            keys: HashMap::new(),
            keystore,
        }
    }

    /// Return the Key that is resolved by a given Address, return Error otherwise
    pub fn find_key(&mut self, addr: &Address) -> Result<Key, Error> {
        if let Some(k) = self.keys.get(&addr) {
            return Ok(k.clone());
        }
        let key_string = format!("wallet-{}", addr.to_string());
        let key_info = self.keystore.get(&key_string)?;
        let new_key = Key::from(key_info.clone());
        self.keys.insert(*addr, new_key.clone());
        Ok(new_key)
    }

    /// Return the resultant Signature after signing a given message
    pub fn sign(&mut self, addr: &Address, msg: &[u8]) -> Result<Signature, Error> {
        let key = self.find_key(addr).map_err(|_| Error::KeyNotExists)?;
        wallet_helpers::sign(*key.key_info.key_type(), key.key_info.private_key(), msg)
    }

    /// Return the KeyInfo for a given Address
    pub fn export(&mut self, addr: &Address) -> Result<KeyInfo, Error> {
        let k = self.find_key(addr)?;
        Ok(k.key_info)
    }

    /// Add Key_Info to the Wallet, return the Address that resolves to this newly added KeyInfo
    pub fn import(&mut self, key_info: &KeyInfo) -> Result<Address, Error> {
        let k = Key::from(key_info.clone());
        let addr = format!("wallet-{}", k.address.to_string());
        self.keystore.put(addr, k.key_info)?;
        Ok(k.address)
    }

    /// Return a Vec that contains all of the Addresses in the Wallet's KeyStore
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

    /// Return the Address of the default KeyInfo in the Wallet
    pub fn get_default(&self) -> Result<Address, Error> {
        let key_info = self.keystore.get(&"default".to_string())?;
        let k = Key::from(key_info.clone());
        Ok(k.address)
    }

    /// Set a default KeyInfo to the Wallet
    pub fn set_default(&mut self, addr: Address) -> Result<(), Error> {
        let addr_string = format!("wallet-{}", addr.to_string());
        let key_info = self.keystore.get(&addr_string)?.clone();
        self.keystore.remove("default".to_string()); // This line should unregister current default key then continue
        self.keystore.put("default".to_string(), key_info)?;
        Ok(())
    }

    /// Generate a new Key that fits the requirement of the given SignatureType
    pub fn generate_key(&mut self, typ: SignatureType) -> Result<Address, Error> {
        let key = generate_key(typ)?;
        let addr = format!("wallet-{}", key.address.to_string());
        self.keystore.put(addr, key.key_info.clone())?;
        self.keys.insert(key.address, key.clone());
        let value = self.keystore.get(&"default".to_string());
        if value.is_err() {
            self.keystore
                .put("default".to_string(), key.key_info.clone())
                .map_err(|err| Error::Other(err.to_string()))?;
        }

        Ok(key.address)
    }

    /// Return whether or not the Wallet contains a Key that is resolved by the supplied Address
    pub fn has_key(&mut self, addr: &Address) -> bool {
        self.find_key(addr).is_ok()
    }
}

/// Generate a new Key that satisfies the given SignatureType
fn generate_key(typ: SignatureType) -> Result<Key, Error> {
    let private_key = wallet_helpers::generate(typ)?;
    let key_info = KeyInfo::new(typ, private_key);
    Ok(Key::from(key_info))
}
