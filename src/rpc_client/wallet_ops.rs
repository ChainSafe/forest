// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ApiInfo, RpcRequest, ServerError};
use crate::{
    key_management::KeyInfo,
    rpc::wallet::*,
    shim::{
        address::Address,
        crypto::{Signature, SignatureType},
    },
};

impl ApiInfo {
    pub async fn wallet_default_address(&self) -> Result<Option<String>, ServerError> {
        self.call(Self::wallet_default_address_req()).await
    }

    pub fn wallet_default_address_req() -> RpcRequest<Option<String>> {
        todo!()
    }

    pub async fn wallet_new(&self, signature_type: SignatureType) -> Result<String, ServerError> {
        self.call(Self::wallet_new_req(signature_type)).await
    }

    pub fn wallet_new_req(signature_type: SignatureType) -> RpcRequest<String> {
        todo!()
    }

    pub async fn wallet_balance(&self, address: String) -> Result<String, ServerError> {
        self.call(Self::wallet_balance_req(address)).await
    }

    pub fn wallet_balance_req(address: String) -> RpcRequest<String> {
        // RpcRequest::new(WALLET_BALANCE, (address,))
        todo!()
    }

    pub async fn wallet_export(&self, address: String) -> Result<KeyInfo, ServerError> {
        self.call(Self::wallet_export_req(address)).await
    }

    pub fn wallet_export_req(address: String) -> RpcRequest<KeyInfo> {
        todo!()
    }

    pub async fn wallet_import(&self, key: Vec<KeyInfo>) -> Result<String, ServerError> {
        self.call(Self::wallet_import_req(key)).await
    }

    pub fn wallet_import_req(key: Vec<KeyInfo>) -> RpcRequest<String> {
        todo!()
    }

    pub async fn wallet_list(&self) -> Result<Vec<Address>, ServerError> {
        self.call(Self::wallet_list_req()).await
    }

    pub fn wallet_list_req() -> RpcRequest<Vec<Address>> {
        todo!()
    }

    pub async fn wallet_has(&self, key: String) -> Result<bool, ServerError> {
        self.call(Self::wallet_has_req(key)).await
    }

    pub fn wallet_has_req(key: String) -> RpcRequest<bool> {
        todo!()
    }

    pub async fn wallet_set_default(&self, address: Address) -> Result<(), ServerError> {
        self.call(Self::wallet_set_default_req(address)).await
    }

    pub fn wallet_set_default_req(address: Address) -> RpcRequest<()> {
        RpcRequest::new(WALLET_SET_DEFAULT, (address,))
    }

    pub async fn wallet_sign(
        &self,
        address: Address,
        data: Vec<u8>,
    ) -> Result<Signature, ServerError> {
        self.call(Self::wallet_sign_req(address, data)).await
    }

    pub fn wallet_sign_req(address: Address, data: Vec<u8>) -> RpcRequest<Signature> {
        RpcRequest::new(WALLET_SIGN, (address, data))
    }

    pub async fn wallet_validate_address(&self, address: String) -> Result<Address, ServerError> {
        self.call(Self::wallet_validate_address_req(address)).await
    }

    pub fn wallet_validate_address_req(address: String) -> RpcRequest<Address> {
        RpcRequest::new(WALLET_VALIDATE_ADDRESS, (address,))
    }

    pub async fn wallet_verify(
        &self,
        address: Address,
        data: Vec<u8>,
        signature: Signature,
    ) -> Result<bool, ServerError> {
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

    pub async fn wallet_delete(&self, address: String) -> Result<(), ServerError> {
        self.call(Self::wallet_delete_req(address)).await
    }

    pub fn wallet_delete_req(address: String) -> RpcRequest<()> {
        RpcRequest::new(WALLET_DELETE, (address,))
    }
}
