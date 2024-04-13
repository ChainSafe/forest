// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{convert::TryFrom, str::FromStr};

use crate::key_management::{Key, KeyInfo};
use crate::lotus_json::LotusJson;
use crate::rpc::{
    reflect::SelfDescribingRpcModule, ApiVersion, Ctx, RPCState, RpcMethod, RpcMethodExt as _,
    ServerError,
};
use crate::shim::{
    address::Address,
    crypto::{Signature, SignatureType},
    econ::TokenAmount,
    state_tree::StateTree,
};
use anyhow::{Context, Result};
use base64::{prelude::BASE64_STANDARD, Engine};
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::types::Params;
use num_traits::Zero;
use schemars::JsonSchema;

macro_rules! for_each_method {
    ($callback:ident) => {
        $callback!(crate::rpc::wallet::WalletBalance);
    };
}
pub(crate) use for_each_method;

pub enum WalletBalance {}
impl RpcMethod<1> for WalletBalance {
    const NAME: &'static str = "Filecoin.WalletBalance";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<Address>,);
    type Ok = LotusJson<TokenAmount>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(address),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let heaviest_ts = ctx.state_manager.chain_store().heaviest_tipset();
        let cid = heaviest_ts.parent_state();

        Ok(LotusJson(
            StateTree::new_from_root(ctx.state_manager.blockstore_owned(), cid)?
                .get_actor(&address)?
                .map(|it| it.balance.clone().into())
                .unwrap_or_default(),
        ))
    }
}

pub enum WalletDefaultAddress {}
impl RpcMethod<0> for WalletDefaultAddress {
    const NAME: &'static str = "Filecoin.WalletDefaultAddress";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = ();
    type Ok = LotusJson<Option<Address>>;

    async fn handle(ctx: Ctx<impl Blockstore>, (): Self::Params) -> Result<Self::Ok, ServerError> {
        let keystore = ctx.keystore.read().await;
        Ok(LotusJson(crate::key_management::get_default(&keystore)?))
    }
}

pub enum WalletExport {}
impl RpcMethod<1> for WalletExport {
    const NAME: &'static str = "Filecoin.WalletExport";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<Address>,);
    type Ok = LotusJson<KeyInfo>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(address),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let keystore = ctx.keystore.read().await;

        let key_info = crate::key_management::export_key_info(&address, &keystore)?;
        Ok(key_info.into())
    }
}

pub enum WalletHas {}
impl RpcMethod<1> for WalletHas {
    const NAME: &'static str = "Filecoin.WalletHas";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<Address>,);
    type Ok = bool;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(address),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let keystore = ctx.keystore.read().await;
        Ok(crate::key_management::find_key(&address, &keystore).is_ok())
    }
}

pub const WALLET_IMPORT: &str = "Filecoin.WalletImport";
/// Import `KeyInfo` to the Wallet, return the Address that corresponds to it
pub async fn wallet_import<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<String, ServerError> {
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
        Err(error.into())
    } else {
        Ok(key.address.to_string())
    }
}

pub const WALLET_LIST: &str = "Filecoin.WalletList";
/// List all Addresses in the Wallet
pub async fn wallet_list<DB: Blockstore>(
    _params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Vec<Address>>, ServerError> {
    let keystore = data.keystore.read().await;
    Ok(crate::key_management::list_addrs(&keystore)?.into())
}

pub const WALLET_NEW: &str = "Filecoin.WalletNew";
/// Generate a new Address that is stored in the Wallet
pub async fn wallet_new<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<String, ServerError> {
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

pub const WALLET_SET_DEFAULT: &str = "Filecoin.WalletSetDefault";
/// Set the default Address for the Wallet
pub async fn wallet_set_default<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<(), ServerError> {
    let LotusJson((address,)): LotusJson<(Address,)> = params.parse()?;

    let mut keystore = data.keystore.write().await;

    let addr_string = format!("wallet-{}", address);
    let key_info = keystore.get(&addr_string)?;
    keystore.remove("default")?; // This line should unregister current default key then continue
    keystore.put("default", key_info)?;
    Ok(())
}

pub const WALLET_SIGN: &str = "Filecoin.WalletSign";
/// Sign a vector of bytes
pub async fn wallet_sign<DB>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Signature>, ServerError>
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

pub const WALLET_VALIDATE_ADDRESS: &str = "Filecoin.WalletValidateAddress";
/// Validates whether a given string can be decoded as a well-formed address
pub(in crate::rpc) async fn wallet_validate_address(
    params: Params<'_>,
) -> Result<LotusJson<Address>, ServerError> {
    let (addr_str,): (String,) = params.parse()?;

    let addr = Address::from_str(&addr_str)?;
    Ok(addr.into())
}

pub const WALLET_VERIFY: &str = "Filecoin.WalletVerify";
/// Verify a Signature, true if verified, false otherwise
pub async fn wallet_verify(params: Params<'_>) -> Result<bool, ServerError> {
    let LotusJson((address, msg, sig)): LotusJson<(Address, Vec<u8>, Signature)> =
        params.parse()?;

    Ok(sig.verify(&msg, &address).is_ok())
}

pub const WALLET_DELETE: &str = "Filecoin.WalletDelete";
/// Deletes a wallet given its address.
pub async fn wallet_delete<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<(), ServerError> {
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
