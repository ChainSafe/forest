// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]

use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient;
use jsonrpsee::transport::TransportClient;
use jsonrpc_v2::{Error as JsonRpcError};
use cid::{json::CidJson, Cid};
use blocks::{
    tipset_json::TipsetJson, header::json::BlockHeaderJson
};
use message::{
    unsigned_message::{json::UnsignedMessageJson},
};
use wallet::{json::KeyInfoJson};
use crypto::signature::json::SignatureJson;

// TODO return result with json err?
jsonrpsee::rpc_api! {
    pub Filecoin {
        /// Chain
        #[rpc(method = "Filecoin.ChainGetGenesis")]
        fn chain_get_genesis() -> TipsetJson;

        #[rpc(method = "Filecoin.ChainGetMessage")]
        fn chain_get_messages(cid: Cid) -> UnsignedMessageJson;

        #[rpc(method = "Filecoin.ChainHead")]
        fn chain_get_head() -> TipsetJson;

        #[rpc(method = "Filecoin.ChainGetBlock")]
        fn chain_get_block(cid: String) -> BlockHeaderJson;

        #[rpc(method = "Filecoin.ChainGetObj")]
        fn chain_read_obj(cid: String) -> Vec<u8>;
    }
}

const URL: &str = "http://127.0.0.1:1234/rpc/v0";

// TODO pass config for url
// pub fn new_client() -> RawClient<R> 
// where
//     R: TransportClient
// {
//     RawClient::new(HttpTransportClient::new(URL))
// }