// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::any::Any;

use crate::key_management::{Key, KeyInfo};
use crate::message::SignedMessage;
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError};
use crate::shim::{
    address::Address,
    crypto::{Signature, SignatureType},
    econ::TokenAmount,
    message::Message,
    state_tree::StateTree,
};
use enumflags2::BitFlags;
use fvm_ipld_blockstore::Blockstore;

pub enum WalletBalance {}
impl RpcMethod<1> for WalletBalance {
    const NAME: &'static str = "Filecoin.WalletBalance";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the balance of a wallet.");

    type Params = (Address,);
    type Ok = TokenAmount;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (address,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let heaviest_ts = ctx.chain_store().heaviest_tipset();
        let cid = heaviest_ts.parent_state();

        Ok(StateTree::new_from_root(ctx.store_owned(), cid)?
            .get_actor(&address)?
            .map(|it| it.balance.clone().into())
            .unwrap_or_default())
    }
}

pub enum WalletDefaultAddress {}
impl RpcMethod<0> for WalletDefaultAddress {
    const NAME: &'static str = "Filecoin.WalletDefaultAddress";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = Option<Address>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let keystore = ctx.keystore.read();
        Ok(crate::key_management::get_default(&keystore)?)
    }
}

pub enum WalletExport {}
impl RpcMethod<1> for WalletExport {
    const NAME: &'static str = "Filecoin.WalletExport";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Admin;

    type Params = (Address,);
    type Ok = KeyInfo;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (address,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let keystore = ctx.keystore.read();
        let key_info = crate::key_management::export_key_info(&address, &keystore)?;
        Ok(key_info)
    }
}

pub enum WalletHas {}
impl RpcMethod<1> for WalletHas {
    const NAME: &'static str = "Filecoin.WalletHas";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;
    const DESCRIPTION: Option<&'static str> =
        Some("Indicates whether the given address exists in the wallet.");

    type Params = (Address,);
    type Ok = bool;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (address,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let keystore = ctx.keystore.read();
        Ok(crate::key_management::find_key(&address, &keystore).is_ok())
    }
}

pub enum WalletImport {}
impl RpcMethod<1> for WalletImport {
    const NAME: &'static str = "Filecoin.WalletImport";
    const PARAM_NAMES: [&'static str; 1] = ["key"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Admin;

    type Params = (KeyInfo,);
    type Ok = Address;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (key_info,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let key = Key::try_from(key_info)?;

        let addr = format!("wallet-{}", key.address);

        let mut keystore = ctx.keystore.write();
        keystore.put(&addr, key.key_info)?;
        Ok(key.address)
    }
}

pub enum WalletList {}
impl RpcMethod<0> for WalletList {
    const NAME: &'static str = "Filecoin.WalletList";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns a list of all addresses in the wallet.");

    type Params = ();
    type Ok = Vec<Address>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let keystore = ctx.keystore.read();
        Ok(crate::key_management::list_addrs(&keystore)?)
    }
}

pub enum WalletNew {}
impl RpcMethod<1> for WalletNew {
    const NAME: &'static str = "Filecoin.WalletNew";
    const PARAM_NAMES: [&'static str; 1] = ["signature_type"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;

    type Params = (SignatureType,);
    type Ok = Address;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (signature_type,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mut keystore = ctx.keystore.write();
        let key = crate::key_management::generate_key(signature_type)?;

        let addr = format!("wallet-{}", key.address);
        keystore.put(&addr, key.key_info.clone())?;
        let value = keystore.get("default");
        if value.is_err() {
            keystore.put("default", key.key_info)?
        }

        Ok(key.address)
    }
}

pub enum WalletSetDefault {}
impl RpcMethod<1> for WalletSetDefault {
    const NAME: &'static str = "Filecoin.WalletSetDefault";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;

    type Params = (Address,);
    type Ok = ();

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (address,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mut keystore = ctx.keystore.write();
        let addr_string = format!("wallet-{address}");
        let key_info = keystore.get(&addr_string)?;
        keystore.remove("default")?; // This line should unregister current default key then continue
        keystore.put("default", key_info)?;
        Ok(())
    }
}

pub enum WalletSign {}
impl RpcMethod<2> for WalletSign {
    const NAME: &'static str = "Filecoin.WalletSign";
    const PARAM_NAMES: [&'static str; 2] = ["address", "message"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Sign;
    const DESCRIPTION: Option<&'static str> =
        Some("Signs the given bytes using the specified address.");

    type Params = (Address, Vec<u8>);
    type Ok = Signature;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, message): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let heaviest_tipset = ctx.chain_store().heaviest_tipset();
        let key_addr = ctx
            .state_manager
            .resolve_to_key_addr(&address, &heaviest_tipset)
            .await?;
        let keystore = &mut *ctx.keystore.write();
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
            &message,
        )?;

        Ok(sig)
    }
}

pub enum WalletSignMessage {}
impl RpcMethod<2> for WalletSignMessage {
    const NAME: &'static str = "Filecoin.WalletSignMessage";
    const PARAM_NAMES: [&'static str; 2] = ["address", "message"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Sign;
    const DESCRIPTION: Option<&'static str> =
        Some("Signs the given message using the specified address.");

    type Params = (Address, Message);
    type Ok = SignedMessage;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, message): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().heaviest_tipset();
        let key_addr = ctx
            .state_manager
            .resolve_to_deterministic_address(address, &ts)
            .await?;

        let keystore = &mut *ctx.keystore.write();
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
            message.cid().to_bytes().as_slice(),
        )?;

        // Could use `SignedMessage::new_unchecked` here but let's make sure
        // we're actually signing the message as expected.
        let smsg = SignedMessage::new_from_parts(message, sig).expect(
            "This is infallible. We just generated the signature, so it cannot be invalid.",
        );

        Ok(smsg)
    }
}

pub enum WalletValidateAddress {}
impl RpcMethod<1> for WalletValidateAddress {
    const NAME: &'static str = "Filecoin.WalletValidateAddress";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (String,);
    type Ok = Address;

    async fn handle(
        _: Ctx<impl Any>,
        (s,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(s.parse()?)
    }
}

pub enum WalletVerify {}
impl RpcMethod<3> for WalletVerify {
    const NAME: &'static str = "Filecoin.WalletVerify";
    const PARAM_NAMES: [&'static str; 3] = ["address", "message", "signature"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (Address, Vec<u8>, Signature);
    type Ok = bool;

    async fn handle(
        _: Ctx<impl Any>,
        (address, message, signature): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(signature.verify(&message, &address).is_ok())
    }
}

pub enum WalletDelete {}
impl RpcMethod<1> for WalletDelete {
    const NAME: &'static str = "Filecoin.WalletDelete";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;

    type Params = (Address,);
    type Ok = ();

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (address,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mut keystore = ctx.keystore.write();
        crate::key_management::remove_key(&address, &mut keystore)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{KeyStore, shim::crypto::SignatureType};

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
        assert!(
            crate::key_management::get_default(&keystore)
                .unwrap()
                .is_none()
        );
    }
}
