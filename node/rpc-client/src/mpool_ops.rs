// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::Filecoin;
use cid::{json::vec::CidJsonVec, Cid};
use jsonrpc_v2::Error as JsonRpcError;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient as HTC;
use message::SignedMessage;

pub async fn pending(
    client: &mut RawClient<HTC>,
    cid: Cid,
) -> Result<Vec<SignedMessage>, JsonRpcError> {
    Ok(Filecoin::mpool_pending(client, CidJsonVec(vec![cid])).await?)
}
