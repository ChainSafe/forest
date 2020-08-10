// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(clippy::all)]
#![allow(unused_variables, dead_code)]

use blocks::gossip_block::json::GossipBlockJson;
use blocks::{header::json::BlockHeaderJson, tipset_json::TipsetJson};
use cid::json::CidJson;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient;
use message::unsigned_message::json::UnsignedMessageJson;
use rpc::RPCSyncState;

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

        /// Sync
        #[rpc(method = "Filecoin.SyncState")]
        fn status() -> RPCSyncState ;

        #[rpc(method = "Filecoin.SyncMarkBad", positional_params)]
        fn mark_bad( params : CidJson)  -> ();

        #[rpc(method = "Filecoin.SyncCheckBad", positional_params)]
        fn check_bad(params : CidJson)  -> String;

        #[rpc(method = "Filecoin.SyncSubmitBlock", positional_params)]
        fn submit_block(params : GossipBlockJson) ;
    }
}

// TODO need to handle dynamic port
const URL: &str = "http://127.0.0.1:1234/rpc/v0";

// TODO pass config for URL
pub fn new_client() -> RawClient<HttpTransportClient> {
    RawClient::new(HttpTransportClient::new(URL))
}
