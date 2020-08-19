// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RpcState;

use address::Address;
use blockstore::BlockStore;
use chain::get_heaviest_tipset;
use crypto::{signature::json::SignatureJson, SignatureType};
use encoding::Cbor;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use message::{
    signed_message::json::SignedMessageJson, unsigned_message::json::UnsignedMessageJson,
    SignedMessage,
};
use num_bigint::BigUint;
use state_tree::StateTree;
use std::convert::TryFrom;
use std::str::FromStr;
use wallet::{json::KeyInfoJson, Key, KeyStore};

/// Return the balance from StateManager for a given Address
pub(crate) async fn wallet_balance<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(String,)>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (addr_str,) = params;
    let address = Address::from_str(&addr_str)?;

    let heaviest_ts = get_heaviest_tipset(data.state_manager.get_block_store_ref())?
        .ok_or("No heaviest tipset")?;
    let cid = heaviest_ts.parent_state();

    let state = StateTree::new_from_root(data.state_manager.get_block_store_ref(), &cid)?;
    match state.get_actor(&address) {
        Ok(act) => {
            let actor = act.ok_or("Could not find actor")?;
            let actor_balance = actor.balance;
            Ok(actor_balance.to_string())
        }
        Err(e) => {
            if e == "Address not found" {
                return Ok(BigUint::default().to_string());
            }
            Err(e.into())
        }
    }
}

/// Get the default Address for the Wallet
pub(crate) async fn wallet_default_address<DB, KS>(
    data: Data<RpcState<DB, KS>>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let keystore = data.keystore.read().await;

    let addr = wallet::get_default(&*keystore)?;
    Ok(addr.to_string())
}

/// Export KeyInfo from the Wallet given its address
pub(crate) async fn wallet_export<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(String,)>,
) -> Result<KeyInfoJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (addr_str,) = params;
    let addr = Address::from_str(&addr_str)?;

    let keystore = data.keystore.read().await;

    let key_info = wallet::export_key_info(&addr, &*keystore)?;
    Ok(KeyInfoJson(key_info))
}

/// Return whether or not a Key is in the Wallet
pub(crate) async fn wallet_has<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(String,)>,
) -> Result<bool, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (addr_str,) = params;
    let addr = Address::from_str(&addr_str)?;

    let keystore = data.keystore.read().await;

    let key = wallet::find_key(&addr, &*keystore).is_ok();
    Ok(key)
}

/// Import Keyinfo to the Wallet, return the Address that corresponds to it
pub(crate) async fn wallet_import<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(KeyInfoJson,)>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (KeyInfoJson(key_info),) = params;

    let key = Key::try_from(key_info)?;

    let addr = format!("wallet-{}", key.address.to_string());

    let mut keystore = data.keystore.write().await;

    keystore.put(addr, key.key_info)?;

    Ok(key.address.to_string())
}

/// List all Addresses in the Wallet
pub(crate) async fn wallet_list<DB, KS>(
    data: Data<RpcState<DB, KS>>,
) -> Result<Vec<String>, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let keystore = data.keystore.read().await;
    let addr_vec = wallet::list_addrs(&*keystore)?;
    let ret = addr_vec.into_iter().map(|a| a.to_string()).collect();
    Ok(ret)
}

/// Generate a new Address that is stored in the Wallet
pub(crate) async fn wallet_new<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(u8,)>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (sig_raw,) = params;
    let sig_type: SignatureType = serde_json::from_str(&sig_raw.to_string())?;
    let mut keystore = data.keystore.write().await;
    let key = wallet::generate_key(sig_type)?;

    let addr = format!("wallet-{}", key.address.to_string());
    keystore.put(addr, key.key_info.clone())?;
    let value = keystore.get(&"default".to_string());
    if value.is_err() {
        keystore.put("default".to_string(), key.key_info)?
    }

    Ok(key.address.to_string())
}

/// Set the default Address for the Wallet
pub(crate) async fn wallet_set_default<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(String,)>,
) -> Result<(), JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (address,) = params;
    let mut keystore = data.keystore.write().await;

    let addr_string = format!("wallet-{}", address);
    let key_info = keystore.get(&addr_string)?;
    keystore.remove("default".to_string())?; // This line should unregister current default key then continue
    keystore.put("default".to_string(), key_info)?;
    Ok(())
}

/// Sign a vector of bytes
pub(crate) async fn wallet_sign<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(String, String)>,
) -> Result<SignatureJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (addr_str, msg_string) = params;

    let address = Address::from_str(&addr_str)?;
    let msg = Vec::from(msg_string);

    let keystore = data.keystore.write().await;

    let key = wallet::find_key(&address, &*keystore)?;

    let sig = wallet::sign(
        *key.key_info.key_type(),
        key.key_info.private_key(),
        msg.as_slice(),
    )?;

    Ok(SignatureJson(sig))
}

/// Sign an UnsignedMessage, return SignedMessage
pub(crate) async fn wallet_sign_message<DB, KS>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(String, UnsignedMessageJson)>,
) -> Result<SignedMessageJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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
pub(crate) async fn wallet_verify<DB, KS>(
    _data: Data<RpcState<DB, KS>>,
    Params(params): Params<(String, String, SignatureJson)>,
) -> Result<bool, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
{
    let (addr_str, msg_str, SignatureJson(sig)) = params;
    let address = Address::from_str(&addr_str)?;
    let msg = Vec::from(msg_str);

    let ret = sig.verify(&msg, &address).is_ok();
    Ok(ret)
}
