// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use super::{wallet_helpers, KeyInfo, KeyStore};
use address::Address;
use crypto::{Signature, SignatureType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::str::FromStr;

/// A Key, this contains a key_info, address, and public_key which holds the key type and private key
#[derive(Clone, PartialEq, Debug, Eq, Serialize, Deserialize)]
pub struct Key {
    pub key_info: KeyInfo,
    // Vec<u8> is used because The public keys for BLS and SECP256K1 are not of the same type
    pub public_key: Vec<u8>,
    pub address: Address,
}

impl TryFrom<KeyInfo> for Key {
    type Error = crate::errors::Error;

    fn try_from(key_info: KeyInfo) -> Result<Self, Self::Error> {
        let public_key = wallet_helpers::to_public(*key_info.key_type(), key_info.private_key())?;
        let address = wallet_helpers::new_address(*key_info.key_type(), &public_key)?;
        Ok(Key {
            key_info,
            public_key,
            address,
        })
    }
}

/// This is a Wallet, it contains 2 HashMaps:
/// - keys which is a HashMap of Keys resolved by their Address
/// - keystore which is a HashMap of KeyInfos resolved by their Address
#[derive(Clone, PartialEq, Debug, Eq)]
pub struct Wallet {
    keys: HashMap<Address, Key>,
    keystore: KeyStore,
}

impl Wallet {
    /// Return a new Wallet with a given KeyStore
    pub fn new(keystore: KeyStore) -> Self {
        Wallet {
            keys: HashMap::new(),
            keystore,
        }
    }

    /// Return a wallet from a given amount of keys. This wallet will not use the
    /// generic keystore trait, but rather specifically use a MemKeyStore
    pub fn new_from_keys(keystore: KeyStore, key_vec: impl IntoIterator<Item = Key>) -> Self {
        let mut keys: HashMap<Address, Key> = HashMap::new();
        for item in key_vec.into_iter() {
            keys.insert(item.address, item);
        }
        Wallet { keys, keystore }
    }

    /// Return the Key that is resolved by a given Address,
    /// If this key does not exist in the keys hashmap, check if this key is in
    /// the keystore, if it is, then add it to keys, otherwise return Error
    pub fn find_key(&mut self, addr: &Address) -> Result<Key, Error> {
        if let Some(k) = self.keys.get(&addr) {
            return Ok(k.clone());
        }
        let key_string = format!("wallet-{}", addr.to_string());
        let key_info = match self.keystore.get(&key_string) {
            Ok(k) => k,
            Err(_) => {
                // replace with testnet prefix
                self.keystore
                    .get(&format!("wallet-t{}", &addr.to_string()[1..]))?
            }
        };
        let new_key = Key::try_from(key_info)?;
        self.keys.insert(*addr, new_key.clone());
        Ok(new_key)
    }

    /// Return the resultant Signature after signing a given message
    pub fn sign(&mut self, addr: &Address, msg: &[u8]) -> Result<Signature, Error> {
        // this will return an error if the key cannot be found in either the keys hashmap or it
        // is not found in the keystore
        let key = self.find_key(addr).map_err(|_| Error::KeyNotExists)?;
        wallet_helpers::sign(*key.key_info.key_type(), key.key_info.private_key(), msg)
    }

    /// Return the KeyInfo for a given Address
    pub fn export(&mut self, addr: &Address) -> Result<KeyInfo, Error> {
        let k = self.find_key(addr)?;
        Ok(k.key_info)
    }

    /// Add Key_Info to the Wallet, return the Address that resolves to this newly added KeyInfo
    pub fn import(&mut self, key_info: KeyInfo) -> Result<Address, Error> {
        let k = Key::try_from(key_info)?;
        let addr = format!("wallet-{}", k.address.to_string());
        self.keystore.put(addr, k.key_info)?;
        Ok(k.address)
    }

    /// Return a Vec that contains all of the Addresses in the Wallet's KeyStore
    pub fn list_addrs(&self) -> Result<Vec<Address>, Error> {
        list_addrs(&self.keystore)
    }

    /// Return the Address of the default KeyInfo in the Wallet
    pub fn get_default(&self) -> Result<Address, Error> {
        let key_info = self.keystore.get(&"default".to_string())?;
        let k = Key::try_from(key_info)?;
        Ok(k.address)
    }

    /// Set a default KeyInfo to the Wallet
    pub fn set_default(&mut self, addr: Address) -> Result<(), Error> {
        let addr_string = format!("wallet-{}", addr.to_string());
        let key_info = self.keystore.get(&addr_string)?;
        if self.keystore.get("default").is_ok() {
            self.keystore.remove("default".to_string())?; // This line should unregister current default key then continue
        }
        self.keystore.put("default".to_string(), key_info)?;
        Ok(())
    }

    /// Generate a new Address that fits the requirement of the given SignatureType
    pub fn generate_addr(&mut self, typ: SignatureType) -> Result<Address, Error> {
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

/// Return the default Address for KeyStore
pub fn get_default(keystore: &KeyStore) -> Result<Address, Error> {
    let key_info = keystore.get(&"default".to_string())?;
    let k = Key::try_from(key_info)?;
    Ok(k.address)
}

/// Return Vec of Addresses sorted by their string representation in KeyStore
pub fn list_addrs(keystore: &KeyStore) -> Result<Vec<Address>, Error> {
    let mut all = keystore.list();
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

/// Return Key corresponding to given Address in KeyStore
pub fn find_key(addr: &Address, keystore: &KeyStore) -> Result<Key, Error> {
    let key_string = format!("wallet-{}", addr.to_string());
    let key_info = keystore.get(&key_string)?;
    let new_key = Key::try_from(key_info)?;
    Ok(new_key)
}

pub fn try_find(addr: &Address, keystore: &mut KeyStore) -> Result<KeyInfo, Error> {
    let key_string = format!("wallet-{}", addr.to_string());
    match keystore.get(&key_string) {
        Ok(k) => Ok(k),
        Err(_) => {
            let mut new_addr = addr.to_string();

            // Try to replace prefix with testnet, for backwards compatibility
            // * We might be able to remove this, look into variants
            new_addr.replace_range(0..1, "t");
            let key_string = format!("wallet-{}", new_addr);
            let key_info = match keystore.get(&key_string) {
                Ok(k) => k,
                Err(_) => keystore.get(&format!("wallet-f{}", &new_addr[1..]))?,
            };
            Ok(key_info)
        }
    }
}

/// Return keyInfo for given Address in KeyStore
pub fn export_key_info(addr: &Address, keystore: &KeyStore) -> Result<KeyInfo, Error> {
    let key = find_key(addr, keystore)?;
    Ok(key.key_info)
}

/// Generate new Key of given SignatureType
pub fn generate_key(typ: SignatureType) -> Result<Key, Error> {
    let private_key = wallet_helpers::generate(typ)?;
    let key_info = KeyInfo::new(typ, private_key);
    Key::try_from(key_info)
}

/// Import KeyInfo into KeyStore
pub fn import(key_info: KeyInfo, keystore: &mut KeyStore) -> Result<Address, Error> {
    let k = Key::try_from(key_info)?;
    let addr = format!("wallet-{}", k.address.to_string());
    keystore.put(addr, k.key_info)?;
    Ok(k.address)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generate, KeyStoreConfig};
    use encoding::blake2b_256;
    use secp256k1::{Message as SecpMessage, SecretKey as SecpPrivate};

    fn construct_priv_keys() -> Vec<Key> {
        let mut secp_keys = Vec::new();
        let mut bls_keys = Vec::new();
        for _ in 1..5 {
            let secp_priv_key = generate(SignatureType::Secp256k1).unwrap();
            let secp_key_info = KeyInfo::new(SignatureType::Secp256k1, secp_priv_key);
            let secp_key = Key::try_from(secp_key_info).unwrap();
            secp_keys.push(secp_key);

            let bls_priv_key = generate(SignatureType::BLS).unwrap();
            let bls_key_info = KeyInfo::new(SignatureType::BLS, bls_priv_key);
            let bls_key = Key::try_from(bls_key_info).unwrap();
            bls_keys.push(bls_key);
        }

        secp_keys.append(bls_keys.as_mut());
        secp_keys
    }

    fn generate_wallet() -> Wallet {
        let key_vec = construct_priv_keys();
        let wallet = Wallet::new_from_keys(KeyStore::new(KeyStoreConfig::Memory).unwrap(), key_vec);
        wallet
    }

    #[test]
    fn contains_key() {
        let key_vec = construct_priv_keys();
        let found_key = key_vec[0].clone();
        let addr = key_vec[0].address;

        let mut wallet =
            Wallet::new_from_keys(KeyStore::new(KeyStoreConfig::Memory).unwrap(), key_vec);

        // make sure that this address resolves to the right key
        assert_eq!(wallet.find_key(&addr).unwrap(), found_key);
        // make sure that has_key returns true as well
        assert_eq!(wallet.has_key(&addr), true);

        let new_priv_key = generate(SignatureType::BLS).unwrap();
        let pub_key =
            wallet_helpers::to_public(SignatureType::BLS, new_priv_key.as_slice()).unwrap();
        let address = Address::new_bls(pub_key.as_slice()).unwrap();

        // test to see if the new key has been created and added to the wallet
        assert_eq!(wallet.has_key(&address), false);
        // test to make sure that the newly made key cannot be added to the wallet because it is not
        // found in the keystore
        assert_eq!(wallet.find_key(&address).unwrap_err(), Error::KeyInfo);
        // sanity check to make sure that the key has not been added to the wallet
        assert_eq!(wallet.has_key(&address), false);
    }

    #[test]
    fn sign() {
        let key_vec = construct_priv_keys();
        let priv_key_bytes = key_vec[2].key_info.private_key().clone();
        let addr = key_vec[2].address;

        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new_from_keys(keystore, key_vec);
        let msg = [0u8; 64];

        let msg_sig = wallet.sign(&addr, &msg).unwrap();

        let msg_complete = blake2b_256(&msg);
        let message = SecpMessage::parse(&msg_complete);
        let priv_key = SecpPrivate::parse_slice(&priv_key_bytes).unwrap();
        let (sig, recovery_id) = secp256k1::sign(&message, &priv_key);
        let mut new_bytes = [0; 65];
        new_bytes[..64].copy_from_slice(&sig.serialize());
        new_bytes[64] = recovery_id.serialize();
        let actual = Signature::new_secp256k1(new_bytes.to_vec());
        assert_eq!(msg_sig, actual)
    }

    #[test]
    fn import_export() {
        let key_vec = construct_priv_keys();
        let key = key_vec[0].clone();
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new_from_keys(keystore, key_vec);

        let key_info = wallet.export(&key.address).unwrap();
        // test to see if export returns the correct key_info
        assert_eq!(key_info, key.key_info);

        let new_priv_key = generate(SignatureType::Secp256k1).unwrap();
        let pub_key =
            wallet_helpers::to_public(SignatureType::Secp256k1, new_priv_key.as_slice()).unwrap();
        let test_addr = Address::new_secp256k1(pub_key.as_slice()).unwrap();
        let key_info_err = wallet.export(&test_addr).unwrap_err();
        // test to make sure that an error is raised when an incorrect address is added
        assert_eq!(key_info_err, Error::KeyInfo);

        let test_key_info = KeyInfo::new(SignatureType::Secp256k1, new_priv_key);
        // make sure that key_info has been imported to wallet
        assert!(wallet.import(test_key_info.clone()).is_ok());

        let duplicate_error = wallet.import(test_key_info).unwrap_err();
        // make sure that error is thrown when attempted to re-import a duplicate key_info
        assert_eq!(duplicate_error, Error::KeyExists);
    }

    #[test]
    fn list_addr() {
        let key_vec = construct_priv_keys();
        let mut addr_string_vec = Vec::new();

        let mut key_store = KeyStore::new(KeyStoreConfig::Memory).unwrap();

        for i in &key_vec {
            addr_string_vec.push(i.address.to_string());

            let addr_string = format!("wallet-{}", i.address.to_string());
            key_store.put(addr_string, i.key_info.clone()).unwrap();
        }

        addr_string_vec.sort();

        let mut addr_vec = Vec::new();

        for addr in addr_string_vec {
            addr_vec.push(Address::from_str(addr.as_str()).unwrap())
        }

        let wallet = Wallet::new(key_store);

        let test_addr_vec = wallet.list_addrs().unwrap();

        // check to see if the addrs in wallet are the same as the key_vec before it was
        // added to the wallet
        assert_eq!(test_addr_vec, addr_vec);
    }

    #[test]
    fn generate_new_key() {
        let mut wallet = generate_wallet();
        let addr = wallet.generate_addr(SignatureType::BLS).unwrap();
        let key = wallet.keystore.get("default").unwrap();
        // make sure that the newly generated key is the default key - checking by key type
        assert_eq!(&SignatureType::BLS, key.key_type());

        let address = format!("wallet-{}", addr.to_string());

        let key_info = wallet.keystore.get(&address).unwrap();
        let key = wallet.keys.get(&addr).unwrap();

        // these assertions will make sure that the key has actually been added to the wallet
        assert_eq!(key_info.key_type(), &SignatureType::BLS);
        assert_eq!(key.address, addr);
    }

    #[test]
    fn get_set_default() {
        let key_store = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(key_store);
        // check to make sure that there is no default
        assert_eq!(wallet.get_default().unwrap_err(), Error::KeyInfo);

        let new_priv_key = generate(SignatureType::Secp256k1).unwrap();
        let pub_key =
            wallet_helpers::to_public(SignatureType::Secp256k1, new_priv_key.as_slice()).unwrap();
        let test_addr = Address::new_secp256k1(pub_key.as_slice()).unwrap();

        let key_info = KeyInfo::new(SignatureType::Secp256k1, new_priv_key);
        let test_addr_string = format!("wallet-{}", test_addr.to_string());

        wallet.keystore.put(test_addr_string, key_info).unwrap();

        // check to make sure that the set_default function completed without error
        assert!(wallet.set_default(test_addr).is_ok());

        // check to make sure that the test_addr is actually the default addr for the wallet
        assert_eq!(wallet.get_default().unwrap(), test_addr);
    }

    #[test]
    fn secp_verify() {
        let secp_priv_key = generate(SignatureType::Secp256k1).unwrap();
        let secp_key_info = KeyInfo::new(SignatureType::Secp256k1, secp_priv_key);
        let secp_key = Key::try_from(secp_key_info).unwrap();
        let addr = secp_key.address;
        let key_store = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new_from_keys(key_store, vec![secp_key]);

        let msg = [0u8; 64];

        let sig = wallet.sign(&addr, &msg).unwrap();
        sig.verify(&msg, &addr).unwrap();

        // invalid verify check
        let invalid_addr = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        assert!(sig.verify(&msg, &invalid_addr).is_err())
    }

    #[test]
    fn bls_verify_test() {
        let bls_priv_key = generate(SignatureType::BLS).unwrap();
        let bls_key_info = KeyInfo::new(SignatureType::BLS, bls_priv_key);
        let bls_key = Key::try_from(bls_key_info).unwrap();
        let addr = bls_key.address;
        let key_store = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new_from_keys(key_store, vec![bls_key]);

        let msg = [0u8; 64];

        let sig = wallet.sign(&addr, &msg).unwrap();
        sig.verify(&msg, &addr).unwrap();

        // invalid verify check
        let invalid_addr = wallet.generate_addr(SignatureType::BLS).unwrap();
        assert!(sig.verify(&msg, &invalid_addr).is_err())
    }
}
