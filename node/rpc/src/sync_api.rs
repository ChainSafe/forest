// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RpcState;
use blockstore::BlockStore;
use cid::json::vec::CidJsonVec;

use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

pub(crate) async fn check_bad<DB: BlockStore + Send + Sync + 'static>(
    _data: Data<RpcState<DB>>,
    Params(_params): Params<(CidJsonVec,)>,
) -> Result<String, JsonRpcError> {
    todo!()
}
