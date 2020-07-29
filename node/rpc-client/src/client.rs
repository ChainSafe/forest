// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]

use blocks::{header::json::BlockHeaderJson, tipset_json::TipsetJson};
use cid::json::CidJson;
use crypto::signature::json::SignatureJson;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient;
use jsonrpsee::transport::TransportClient;
use message::unsigned_message::json::UnsignedMessageJson;
use wallet::json::KeyInfoJson;

jsonrpsee::rpc_api! {
    pub Filecoin {
        /// Chain
        #[rpc(method = "Filecoin.ChainGetBlock", positional_params)]
        fn chain_get_block(cid: CidJson) -> BlockHeaderJson;

        #[rpc(method = "Filecoin.ChainGetGenesis")]
        fn chain_get_genesis() -> TipsetJson;

        #[rpc(method = "Filecoin.ChainHead")]
        fn chain_get_head() -> TipsetJson;

        #[rpc(method = "Filecoin.ChainGetMessage", positional_params)]
        fn chain_get_messages(cid: CidJson) -> UnsignedMessageJson;

        #[rpc(method = "Filecoin.ChainGetObj", positional_params)]
        fn chain_read_obj(cid: CidJson) -> Vec<u8>;

        /// Wallet
        #[rpc(method = "Filecoin.WalletNew", positional_params)]
        fn wallet_new(sig_type: Vec<u8>) -> String;

        #[rpc(method = "Filecoin.WalletList")]
        fn wallet_list() -> Vec<String>;

        #[rpc(method = "Filecoin.WalletBalance", positional_params)]
        fn wallet_balance(address: String) -> String;

        #[rpc(method = "Filecoin.WalletSetDefault", positional_params)]
        fn wallet_set_default(address: String);

        #[rpc(method = "Filecoin.WalletDefault")]
        fn wallet_default() -> String;

        #[rpc(method = "Filecoin.WalletSign", positional_params)]
        fn wallet_sign(params: (String, String)) -> SignatureJson;

        #[rpc(method = "Filecoin.WalletVerify")]
        fn wallet_verify(params: (String, String, SignatureJson)) -> bool;

        #[rpc(method = "Filecoin.WalletImport", positional_params)]
        fn wallet_import(key_info: KeyInfoJson) -> String;

        #[rpc(method = "Filecoin.WalletExport", positional_params)]
        fn wallet_export(address: String) -> KeyInfoJson;
    }
}

// TODO need to handle dynamic port
const URL: &str = "http://127.0.0.1:1234/rpc/v0";

// TODO pass config for URL
pub fn new_client() -> RawClient<HttpTransportClient> {
    RawClient::new(HttpTransportClient::new(URL))
}
