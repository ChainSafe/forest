// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(clippy::all)]
#![allow(unused_variables, dead_code)]

use std::env;

use blocks::{header::json::BlockHeaderJson, tipset_json::TipsetJson};
use cid::json::CidJson;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient;
use message::unsigned_message::json::UnsignedMessageJson;

jsonrpsee::rpc_api! {
    pub Filecoin {
        /// Auth
        #[rpc(method = "Filecoin.AuthNew", positional_params)]
        fn auth_new(perm: Vec<String>) -> String;
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
    }
}

const URL: &str = "http://127.0.0.1:1234/rpc/v0";
const API_INFO_KEY: &str = "FULLNODE_API_INFO";

pub fn new_client() -> RawClient<HttpTransportClient> {
    let api_info = env::var(API_INFO_KEY).unwrap_or(URL.to_owned());
    RawClient::new(HttpTransportClient::new(URL))
}
