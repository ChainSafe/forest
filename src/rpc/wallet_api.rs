// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]
use std::{convert::TryFrom, str::FromStr};

use crate::key_management::{Key, KeyInfo};
use crate::lotus_json::LotusJson;
use crate::rpc::error::JsonRpseeError;
use crate::rpc_api::data_types::RPCState;
use crate::shim::{
    address::Address,
    crypto::{Signature, SignatureType},
    econ::TokenAmount,
    state_tree::StateTree,
};
use anyhow::{Context, Result};
use base64::{prelude::BASE64_STANDARD, Engine};
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::types::Params as JsonRpseeParams;
use num_traits::Zero;

use std::sync::Arc;

/// Return the balance from `StateManager` for a given `Address`
pub async fn wallet_balance<DB: Blockstore>(
    params: JsonRpseeParams<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<String, JsonRpseeError> {
    let (addr_str,): (String,) = params.parse()?;

    let address = Address::from_str(&addr_str)?;

    let heaviest_ts = data.state_manager.chain_store().heaviest_tipset();
    let cid = heaviest_ts.parent_state();

    let state = StateTree::new_from_root(data.state_manager.blockstore_owned(), cid)?;
    match state.get_actor(&address) {
        Ok(act) => {
            if let Some(actor) = act {
                let actor_balance = &actor.balance;
                Ok(actor_balance.atto().to_string())
            } else {
                Ok(TokenAmount::zero().atto().to_string())
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Get the default Address for the Wallet
pub async fn wallet_default_address<DB: Blockstore>(
    _params: JsonRpseeParams<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<Option<String>, JsonRpseeError> {
    let keystore = data.keystore.read().await;

    let addr = crate::key_management::get_default(&keystore)?;
    Ok(addr.map(|s| s.to_string()))
}

/// Export `KeyInfo` from the Wallet given its address
pub async fn wallet_export<DB: Blockstore>(
    params: JsonRpseeParams<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<LotusJson<KeyInfo>, JsonRpseeError> {
    let (addr_str,): (String,) = params.parse()?;

    let addr = Address::from_str(&addr_str)?;

    let keystore = data.keystore.read().await;

    let key_info = crate::key_management::export_key_info(&addr, &keystore)?;
    Ok(key_info.into())
}

/// Return whether or not a Key is in the Wallet
pub async fn wallet_has<DB: Blockstore>(
    params: JsonRpseeParams<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<bool, JsonRpseeError> {
    let (addr_str,): (String,) = params.parse()?;

    let addr = Address::from_str(&addr_str)?;

    let keystore = data.keystore.read().await;

    let key = crate::key_management::find_key(&addr, &keystore).is_ok();
    Ok(key)
}

/// Import `KeyInfo` to the Wallet, return the Address that corresponds to it
pub async fn wallet_import<DB: Blockstore>(
    params: JsonRpseeParams<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<String, JsonRpseeError> {
    let params: LotusJson<Vec<KeyInfo>> = params.parse()?;

    let key_info = params
        .into_inner()
        .into_iter()
        .next()
        .context("empty vector")?;

    let key = Key::try_from(key_info)?;

    let addr = format!("wallet-{}", key.address);

    let mut keystore = data.keystore.write().await;

    if let Err(error) = keystore.put(&addr, key.key_info) {
        return Err(error.into());
    } else {
        Ok(key.address.to_string())
    }
}

/// List all Addresses in the Wallet
pub async fn wallet_list<DB: Blockstore>(
    _params: JsonRpseeParams<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<LotusJson<Vec<Address>>, JsonRpseeError> {
    let keystore = data.keystore.read().await;
    Ok(crate::key_management::list_addrs(&keystore)?.into())
}

/// Generate a new Address that is stored in the Wallet
pub async fn wallet_new<DB: Blockstore>(
    params: JsonRpseeParams<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<String, JsonRpseeError> {
    let LotusJson((sig_raw,)): LotusJson<(SignatureType,)> = params.parse()?;

    let mut keystore = data.keystore.write().await;
    let key = crate::key_management::generate_key(sig_raw)?;

    let addr = format!("wallet-{}", key.address);
    keystore.put(&addr, key.key_info.clone())?;
    let value = keystore.get("default");
    if value.is_err() {
        keystore.put("default", key.key_info)?
    }

    Ok(key.address.to_string())
}

/// Set the default Address for the Wallet
pub async fn wallet_set_default<DB: Blockstore>(
    params: JsonRpseeParams<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<(), JsonRpseeError> {
    let LotusJson((address,)): LotusJson<(Address,)> = params.parse()?;

    let mut keystore = data.keystore.write().await;

    let addr_string = format!("wallet-{}", address);
    let key_info = keystore.get(&addr_string)?;
    keystore.remove("default")?; // This line should unregister current default key then continue
    keystore.put("default", key_info)?;
    Ok(())
}

/// Sign a vector of bytes
pub async fn wallet_sign<DB>(
    params: JsonRpseeParams<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<LotusJson<Signature>, JsonRpseeError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let LotusJson((address, msg_string)): LotusJson<(Address, Vec<u8>)> = params.parse()?;

    let state_manager = &data.state_manager;
    let heaviest_tipset = data.state_manager.chain_store().heaviest_tipset();
    let key_addr = state_manager
        .resolve_to_key_addr(&address, &heaviest_tipset)
        .await?;
    let keystore = &mut *data.keystore.write().await;
    let key = match crate::key_management::find_key(&key_addr, keystore) {
        Ok(key) => key,
        Err(_) => {
            let key_info = crate::key_management::try_find(&key_addr, keystore)?;
            Key::try_from(key_info)?
        }
    };

    let sig = crate::key_management::sign(
        *key.key_info.key_type(),
        key.key_info.private_key(),
        &BASE64_STANDARD.decode(msg_string)?,
    )?;

    Ok(sig.into())
}

/// Validates whether a given string can be decoded as a well-formed address
pub(in crate::rpc) async fn wallet_validate_address(
    params: JsonRpseeParams<'_>,
) -> Result<LotusJson<Address>, JsonRpseeError> {
    let (addr_str,): (String,) = params.parse()?;

    let addr = Address::from_str(&addr_str)?;
    Ok(addr.into())
}

/// Verify a Signature, true if verified, false otherwise
pub async fn wallet_verify(params: JsonRpseeParams<'_>) -> Result<bool, JsonRpseeError> {
    let LotusJson((address, msg, sig)): LotusJson<(Address, Vec<u8>, Signature)> =
        params.parse()?;

    Ok(sig.verify(&msg, &address).is_ok())
}

/// Deletes a wallet given its address.
pub async fn wallet_delete<DB: Blockstore>(
    params: JsonRpseeParams<'_>,
    data: Arc<Arc<RPCState<DB>>>,
) -> Result<(), JsonRpseeError> {
    let (addr_str,): (String,) = params.parse()?;

    let mut keystore = data.keystore.write().await;
    let addr = Address::from_str(&addr_str)?;
    crate::key_management::remove_key(&addr, &mut keystore)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{shim::crypto::SignatureType, KeyStore};

    #[tokio::test]
    async fn wallet_delete_existing_key() {
        let key = crate::key_management::generate_key(SignatureType::Secp256k1).unwrap();
        let addr = format!("wallet-{}", key.address);
        let mut keystore = KeyStore::new(crate::KeyStoreConfig::Memory).unwrap();
        keystore.put(&addr, key.key_info.clone()).unwrap();
        crate::key_management::remove_key(&key.address, &mut keystore).unwrap();
        assert!(keystore.get(&addr).is_err());
    }

    #[tokio::test]
    async fn wallet_delete_empty_keystore() {
        let key = crate::key_management::generate_key(SignatureType::Secp256k1).unwrap();
        let mut keystore = KeyStore::new(crate::KeyStoreConfig::Memory).unwrap();
        assert!(crate::key_management::remove_key(&key.address, &mut keystore).is_err());
    }

    #[tokio::test]
    async fn wallet_delete_non_existent_key() {
        let key1 = crate::key_management::generate_key(SignatureType::Secp256k1).unwrap();
        let key2 = crate::key_management::generate_key(SignatureType::Secp256k1).unwrap();
        let addr1 = format!("wallet-{}", key1.address);
        let mut keystore = KeyStore::new(crate::KeyStoreConfig::Memory).unwrap();
        keystore.put(&addr1, key1.key_info.clone()).unwrap();
        assert!(crate::key_management::remove_key(&key2.address, &mut keystore).is_err());
    }

    #[tokio::test]
    async fn wallet_delete_default_key() {
        let key1 = crate::key_management::generate_key(SignatureType::Secp256k1).unwrap();
        let key2 = crate::key_management::generate_key(SignatureType::Secp256k1).unwrap();
        let addr1 = format!("wallet-{}", key1.address);
        let addr2 = format!("wallet-{}", key2.address);
        let mut keystore = KeyStore::new(crate::KeyStoreConfig::Memory).unwrap();
        keystore.put(&addr1, key1.key_info.clone()).unwrap();
        keystore.put(&addr2, key2.key_info.clone()).unwrap();
        keystore.put("default", key2.key_info.clone()).unwrap();
        crate::key_management::remove_key(&key2.address, &mut keystore).unwrap();
        assert!(crate::key_management::get_default(&keystore)
            .unwrap()
            .is_none());
    }
}
