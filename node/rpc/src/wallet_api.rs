// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::State;

use address::Address;
use blockstore::BlockStore;
use chain::get_heaviest_tipset;
use crypto::{Signature, SignatureType};
use encoding::Cbor;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use message::{SignedMessage, UnsignedMessage};
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use num_bigint::BigUint;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use state_tree::StateTree;
use std::convert::TryFrom;
use wallet::{Key, KeyInfo, KeyStore};

pub struct Balance {
    pub balance: BigUint,
}

impl Serialize for Balance {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (BigUintSer(&self.balance)).serialize(s)
    }
}

impl<'de> Deserialize<'de> for Balance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let BigUintDe(balance) = Deserialize::deserialize(deserializer)?;
        Ok(Self { balance })
    }
}

/// Generate a new Address that is stored in the wallet
pub(crate) async fn wallet_new<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    data: Data<State<DB, T>>,
    Params(params): Params<(SignatureType,)>,
) -> Result<Address, JsonRpcError> {
    let (sig_type,) = params;
    let mut keystore = data.keystore.write().await;
    let key = wallet::generate_key(sig_type)?;

    let addr = format!("wallet-{}", key.address.to_string());
    keystore.put(addr, key.key_info.clone())?;
    let value = keystore.get(&"default".to_string());
    if value.is_err() {
        keystore.put("default".to_string(), key.key_info)?
    }

    Ok(key.address)
}

/// Return the balance from state manager for a given address
pub(crate) async fn wallet_get_balance<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    data: Data<State<DB, T>>,
    Params(params): Params<(Address,)>,
) -> Result<Balance, JsonRpcError> {
    let (addr,) = params;

    let heaviest_ts = get_heaviest_tipset(data.store.as_ref())?.unwrap();
    let cid = heaviest_ts.parent_state();

    let state = StateTree::new_from_root(data.store.as_ref(), cid)?;
    let actor = state.get_actor(&addr)?.unwrap();
    let actor_balance = actor.balance;
    let balance = Balance {
        balance: actor_balance,
    };
    Ok(balance)
}

pub(crate) async fn wallet_get_default<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    data: Data<State<DB, T>>,
) -> Result<Address, JsonRpcError> {
    let keystore = data.keystore.read().await;

    let addr = wallet::get_default(&*keystore)?;
    Ok(addr)
}

pub(crate) async fn wallet_list_addrs<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    data: Data<State<DB, T>>,
) -> Result<Vec<Address>, JsonRpcError> {
    let keystore = data.keystore.read().await;
    let addr_vec = wallet::list_addrs(&*keystore)?;
    Ok(addr_vec)
}

pub(crate) async fn wallet_export<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    data: Data<State<DB, T>>,
    Params(params): Params<(Address,)>,
) -> Result<KeyInfo, JsonRpcError> {
    let (addr,) = params;

    let keystore = data.keystore.read().await;

    let key_info = wallet::export_key_info(&addr, &*keystore)?;
    Ok(key_info)
}

pub(crate) async fn wallet_has_key<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    data: Data<State<DB, T>>,
    Params(params): Params<(Address,)>,
) -> Result<bool, JsonRpcError> {
    let (addr,) = params;

    let keystore = data.keystore.read().await;

    let key = wallet::find_key(&addr, &*keystore).is_ok();
    Ok(key)
}

pub(crate) async fn wallet_import<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    data: Data<State<DB, T>>,
    Params(params): Params<(KeyInfo,)>,
) -> Result<Address, JsonRpcError> {
    let (key_info,) = params;

    let key = Key::try_from(key_info)?;

    let addr = format!("wallet-{}", key.address.to_string());

    let mut keystore = data.keystore.write().await;

    keystore.put(addr, key.key_info)?;

    Ok(key.address)
}

pub(crate) async fn wallet_sign<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    data: Data<State<DB, T>>,
    Params(params): Params<(Address, Vec<u8>)>,
) -> Result<Signature, JsonRpcError> {
    let (address, msg) = params;

    let keystore = data.keystore.write().await;

    let key = wallet::find_key(&address, &*keystore)?;

    let sig = wallet::sign(
        *key.key_info.key_type(),
        key.key_info.private_key(),
        msg.as_slice(),
    )?;

    Ok(sig)
}

pub(crate) async fn wallet_sign_message<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    data: Data<State<DB, T>>,
    Params(params): Params<(Address, UnsignedMessage)>,
) -> Result<SignedMessage, JsonRpcError> {
    let (address, msg) = params;
    let msg_cid = msg.cid()?;

    let keystore = data.keystore.write().await;

    let key = wallet::find_key(&address, &*keystore)?;

    let sig = wallet::sign(
        *key.key_info.key_type(),
        key.key_info.private_key(),
        msg_cid.to_bytes().as_slice(),
    )?;

    let smsg = SignedMessage::new_from_fields(msg, sig);

    Ok(smsg)
}

pub(crate) async fn wallet_verify<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    _data: Data<State<DB, T>>,
    Params(params): Params<(Address, Vec<u8>, Signature)>,
) -> Result<bool, JsonRpcError> {
    let (address, msg, sig) = params;

    let ret = sig.verify(&msg, &address).is_ok();
    Ok(ret)
}

pub(crate) async fn wallet_set_default<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    data: Data<State<DB, T>>,
    Params(params): Params<(Address,)>,
) -> Result<(), JsonRpcError> {
    let (address,) = params;
    let mut keystore = data.keystore.write().await;

    let addr_string = format!("wallet-{}", address.to_string());
    let key_info = keystore.get(&addr_string)?;
    keystore.remove("default".to_string()); // This line should unregister current default key then continue
    keystore.put("default".to_string(), key_info)?;
    Ok(())
}

pub(crate) async fn wallet_delete<
    DB: BlockStore + Send + Sync + 'static,
    T: KeyStore + Send + Sync + 'static,
>(
    data: Data<State<DB, T>>,
    Params(params): Params<(Address,)>,
) -> Result<(), JsonRpcError> {
    let (address,) = params;
    let mut keystore = data.keystore.write().await;

    let addr_string = format!("wallet-{}", address.to_string());

    keystore.remove(addr_string);

    Ok(())
}
