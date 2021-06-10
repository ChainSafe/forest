// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::filecoin_rpc;
use cid::{json::vec::CidJsonVec, Cid};
use jsonrpc_v2::Error as JsonRpcError;
use message::SignedMessage;

pub async fn pending(cid: Cid) -> Result<Vec<SignedMessage>, JsonRpcError> {
    filecoin_rpc::mpool_pending(CidJsonVec(vec![cid])).await?
}
