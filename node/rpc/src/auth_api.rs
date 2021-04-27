// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RpcState;
use auth::*;
use beacon::Beacon;
use blockstore::BlockStore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use wallet::KeyStore;

/// RPC call to create a new JWT Token
pub(crate) async fn auth_new<'a, DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(Vec<String>,)>,
) -> Result<Vec<u8>, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (perms,) = params;
    let ks = data.keystore.read().await;
    let ki = ks.get(JWT_IDENTIFIER)?;
    let token = create_token(perms, ki.private_key())?;
    Ok(token.as_bytes().to_vec())
}

/// RPC call to verify JWT Token and return the token's permissions
pub(crate) async fn auth_verify<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(String,)>,
) -> Result<Vec<String>, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let ks = data.keystore.read().await;
    let (header_raw,) = params;
    let token = header_raw.trim_start_matches("Bearer ");
    let ki = ks.get(JWT_IDENTIFIER)?;
    let perms = verify_token(&token, ki.private_key())?;
    Ok(perms)
}
