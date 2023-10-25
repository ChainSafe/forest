// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ApiInfo, JsonRpcError, RpcRequest};
use crate::{
    key_management::KeyInfo,
    rpc_api::wallet_api::*,
    shim::{
        address::Address,
        crypto::{Signature, SignatureType},
    },
};

impl ApiInfo {
    pub async fn wallet_default_address(&self) -> Result<Option<String>, JsonRpcError> {
        self.call(Self::wallet_default_address_req()).await
    }

    pub fn wallet_default_address_req() -> RpcRequest<Option<String>> {
        RpcRequest::new(WALLET_DEFAULT_ADDRESS, ())
    }

    pub async fn wallet_new(&self, signature_type: SignatureType) -> Result<String, JsonRpcError> {
        self.call(Self::wallet_new_req(signature_type)).await
    }

    pub fn wallet_new_req(signature_type: SignatureType) -> RpcRequest<String> {
        RpcRequest::new(WALLET_NEW, (signature_type,))
    }

    pub async fn wallet_balance(&self, address: String) -> Result<String, JsonRpcError> {
        self.call(Self::wallet_balance_req(address)).await
    }

    pub fn wallet_balance_req(address: String) -> RpcRequest<String> {
        RpcRequest::new(WALLET_BALANCE, (address,))
    }

    pub async fn wallet_export(&self, address: String) -> Result<KeyInfo, JsonRpcError> {
        self.call(Self::wallet_export_req(address)).await
    }

    pub fn wallet_export_req(address: String) -> RpcRequest<KeyInfo> {
        RpcRequest::new(WALLET_EXPORT, address)
    }

    pub async fn wallet_import(&self, key: Vec<KeyInfo>) -> Result<String, JsonRpcError> {
        self.call(Self::wallet_import_req(key)).await
    }

    pub fn wallet_import_req(key: Vec<KeyInfo>) -> RpcRequest<String> {
        RpcRequest::new(WALLET_IMPORT, key)
    }

    pub async fn wallet_list(&self) -> Result<Vec<Address>, JsonRpcError> {
        self.call(Self::wallet_list_req()).await
    }

    pub fn wallet_list_req() -> RpcRequest<Vec<Address>> {
        RpcRequest::new(WALLET_LIST, ())
    }

    pub async fn wallet_has(&self, key: String) -> Result<bool, JsonRpcError> {
        self.call(Self::wallet_has_req(key)).await
    }

    pub fn wallet_has_req(key: String) -> RpcRequest<bool> {
        RpcRequest::new(WALLET_HAS, key)
    }

    pub async fn wallet_set_default(&self, address: Address) -> Result<(), JsonRpcError> {
        self.call(Self::wallet_set_default_req(address)).await
    }

    pub fn wallet_set_default_req(address: Address) -> RpcRequest<()> {
        RpcRequest::new(WALLET_SET_DEFAULT, (address,))
    }

    pub async fn wallet_sign(
        &self,
        address: Address,
        data: Vec<u8>,
    ) -> Result<Signature, JsonRpcError> {
        self.call(Self::wallet_sign_req(address, data)).await
    }

    pub fn wallet_sign_req(address: Address, data: Vec<u8>) -> RpcRequest<Signature> {
        RpcRequest::new(WALLET_SIGN, (address, data))
    }

    pub async fn wallet_verify(
        &self,
        address: Address,
        data: Vec<u8>,
        signature: Signature,
    ) -> Result<bool, JsonRpcError> {
        self.call(Self::wallet_verify_req(address, data, signature))
            .await
    }

    pub fn wallet_verify_req(
        address: Address,
        data: Vec<u8>,
        signature: Signature,
    ) -> RpcRequest<bool> {
        RpcRequest::new(WALLET_VERIFY, (address, data, signature))
    }

    pub async fn wallet_delete(&self, address: String) -> Result<(), JsonRpcError> {
        self.call(Self::wallet_delete_req(address)).await
    }

    pub fn wallet_delete_req(address: String) -> RpcRequest<()> {
        RpcRequest::new(WALLET_DELETE, address)
    }
}
