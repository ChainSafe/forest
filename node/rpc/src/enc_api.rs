// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

use crate::RpcState;
use beacon::Beacon;
use blockstore::BlockStore;
use wallet::KeyStore;

/// RPC call to derive a keystore encryption key on launch
pub(crate) async fn enc_unlock<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(String,)>,
) -> Result<(), JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (passphrase,) = params;
    let ks = data.keystore.write().await;
    ks.unlock(&passphrase)?
}
