// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RpcState;

use address::{json::AddressJson, Address};
use beacon::Beacon;
use blockstore::BlockStore;
use crypto::signature::json::{signature_type::SignatureTypeJson, SignatureJson};
use encoding::Cbor;
use fil_types::verifier::FullVerifier;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use message::{
    signed_message::json::SignedMessageJson, unsigned_message::json::UnsignedMessageJson,
    SignedMessage,
};
use num_bigint::BigUint;
use state_tree::StateTree;
use std::convert::TryFrom;
use std::str::FromStr;
use wallet::{json::KeyInfoJson, Key};

/// Return the balance from StateManager for a given Address
pub(crate) async fn wallet_balance<DB, B>(
    data: Data<RpcState<DB, B>>,
    Params(params): Params<(String,)>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (addr_str,) = params;
    let address = Address::from_str(&addr_str)?;

    let heaviest_ts = data
        .state_manager
        .chain_store()
        .heaviest_tipset()
        .await
        .ok_or("No heaviest tipset")?;
    let cid = heaviest_ts.parent_state();

    let state = StateTree::new_from_root(data.state_manager.blockstore(), &cid)?;
    match state.get_actor(&address) {
        Ok(act) => {
            if let Some(actor) = act {
                let actor_balance = actor.balance;
                Ok(actor_balance.to_string())
            } else {
                Ok(BigUint::default().to_string())
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Get the default Address for the Wallet
pub(crate) async fn wallet_default_address<DB, B>(
    data: Data<RpcState<DB, B>>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let keystore = data.keystore.read().await;

    let addr = wallet::get_default(&*keystore)?;
    Ok(addr.to_string())
}

/// Export KeyInfo from the Wallet given its address
pub(crate) async fn wallet_export<DB, B>(
    data: Data<RpcState<DB, B>>,
    Params(params): Params<(String,)>,
) -> Result<KeyInfoJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (addr_str,) = params;
    let addr = Address::from_str(&addr_str)?;

    let keystore = data.keystore.read().await;

    let key_info = wallet::export_key_info(&addr, &*keystore)?;
    Ok(KeyInfoJson(key_info))
}

/// Return whether or not a Key is in the Wallet
pub(crate) async fn wallet_has<DB, B>(
    data: Data<RpcState<DB, B>>,
    Params(params): Params<(String,)>,
) -> Result<bool, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (addr_str,) = params;
    let addr = Address::from_str(&addr_str)?;

    let keystore = data.keystore.read().await;

    let key = wallet::find_key(&addr, &*keystore).is_ok();
    Ok(key)
}

/// Import Keyinfo to the Wallet, return the Address that corresponds to it
pub(crate) async fn wallet_import<DB, B>(
    data: Data<RpcState<DB, B>>,
    Params(params): Params<Vec<KeyInfoJson>>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let key_info: wallet::KeyInfo = params.first().cloned().unwrap().into();
    let key = Key::try_from(key_info)?;

    let addr = format!("wallet-{}", key.address.to_string());

    let mut keystore = data.keystore.write().await;
    keystore.put(addr, key.key_info)?;

    Ok(key.address.to_string())
}

/// List all Addresses in the Wallet
pub(crate) async fn wallet_list<DB, B>(
    data: Data<RpcState<DB, B>>,
) -> Result<Vec<AddressJson>, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let keystore = data.keystore.read().await;
    Ok(wallet::list_addrs(&*keystore)?
        .into_iter()
        .map(AddressJson::from)
        .collect())
}

/// Generate a new Address that is stored in the Wallet
pub(crate) async fn wallet_new<DB, B>(
    data: Data<RpcState<DB, B>>,
    Params(params): Params<(SignatureTypeJson,)>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (sig_raw,) = params;
    let mut keystore = data.keystore.write().await;
    let key = wallet::generate_key(sig_raw.0)?;

    let addr = format!("wallet-{}", key.address.to_string());
    keystore.put(addr, key.key_info.clone())?;
    let value = keystore.get(&"default".to_string());
    if value.is_err() {
        keystore.put("default".to_string(), key.key_info)?
    }

    Ok(key.address.to_string())
}

/// Set the default Address for the Wallet
pub(crate) async fn wallet_set_default<DB, B>(
    data: Data<RpcState<DB, B>>,
    Params(params): Params<(AddressJson,)>,
) -> Result<(), JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (address,) = params;
    let mut keystore = data.keystore.write().await;

    let addr_string = format!("wallet-{}", address.0);
    let key_info = keystore.get(&addr_string)?;
    keystore.remove("default".to_string())?; // This line should unregister current default key then continue
    keystore.put("default".to_string(), key_info)?;
    Ok(())
}

/// Sign a vector of bytes
pub(crate) async fn wallet_sign<DB, B>(
    data: Data<RpcState<DB, B>>,
    Params(params): Params<(AddressJson, String)>,
) -> Result<SignatureJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let state_manager = &data.state_manager;
    let (addr, msg_string) = params;
    let address = addr.0;
    let heaviest_tipset = data
        .state_manager
        .chain_store()
        .heaviest_tipset()
        .await
        .ok_or_else(|| "Could not get heaviest tipset".to_string())?;
    let key_addr = state_manager
        .resolve_to_key_addr::<FullVerifier>(&address, &heaviest_tipset)
        .await?;
    let keystore = &mut *data.keystore.write().await;
    let key = match wallet::find_key(&key_addr, keystore) {
        Ok(key) => key,
        Err(_) => {
            let key_info = wallet::try_find(&key_addr, keystore)?;
            Key::try_from(key_info)?
        }
    };

    let sig = wallet::sign(
        *key.key_info.key_type(),
        key.key_info.private_key(),
        &base64::decode(msg_string)?,
    )?;
    Ok(SignatureJson(sig))
}

/// Sign an UnsignedMessage, return SignedMessage
pub(crate) async fn wallet_sign_message<DB, B>(
    data: Data<RpcState<DB, B>>,
    Params(params): Params<(String, UnsignedMessageJson)>,
) -> Result<SignedMessageJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (addr_str, UnsignedMessageJson(msg)) = params;
    let address = Address::from_str(&addr_str)?;
    let msg_cid = msg.cid()?;

    let keystore = data.keystore.write().await;

    let key = wallet::find_key(&address, &*keystore)?;

    let sig = wallet::sign(
        *key.key_info.key_type(),
        key.key_info.private_key(),
        msg_cid.to_bytes().as_slice(),
    )?;

    let smsg = SignedMessage::new_from_parts(msg, sig)?;

    Ok(SignedMessageJson(smsg))
}

/// Verify a Signature, true if verified, false otherwise
pub(crate) async fn wallet_verify<DB, B>(
    _data: Data<RpcState<DB, B>>,
    Params(params): Params<(String, String, SignatureJson)>,
) -> Result<bool, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (addr_str, msg_str, SignatureJson(sig)) = params;
    let address = Address::from_str(&addr_str)?;
    let msg = Vec::from(msg_str);

    let ret = sig.verify(&msg, &address).is_ok();
    Ok(ret)
}
