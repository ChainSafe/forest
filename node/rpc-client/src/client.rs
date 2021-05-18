// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::{Error as JsonRpcError, Id, RequestObject};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use std::env;

const DEFUALT_URL: &str = "http://127.0.0.1:1234/rpc/v0";
const API_INFO_KEY: &str = "FULLNODE_API_INFO";

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
struct JsonRpcResponse<T> {
    pub jsonrpc: String,
    pub result: T,
    pub id: Option<Id>,
}

pub async fn call<R>(rpc_call: RequestObject) -> Result<R, JsonRpcError>
where
    R: DeserializeOwned,
{
    let url = env::var(API_INFO_KEY).unwrap_or(DEFUALT_URL.to_owned());

    let mut http_res = surf::post(url)
        .body(surf::Body::from_json(&rpc_call)?)
        .await?;

    let result = http_res.body_string().await?;

    match serde_json::from_str::<JsonRpcResponse<R>>(&result) {
        Ok(r) => Ok(r.result),
        Err(e) => Err(jsonrpc_v2::Error::from(e)),
    }
}

pub async fn call_method<R>(method_name: &str) -> Result<R, JsonRpcError>
where
    R: DeserializeOwned,
{
    let rpc_call = jsonrpc_v2::RequestObject::request()
        .with_method(method_name)
        .finish();

    call(rpc_call).await
}

pub async fn call_params<P, R>(method_name: &str, params: P) -> Result<R, JsonRpcError>
where
    P: Into<Value>,
    R: DeserializeOwned,
{
    let rpc_call = jsonrpc_v2::RequestObject::request()
        .with_method(method_name)
        .with_params(params)
        .finish();

    call(rpc_call).await
}

pub mod filecoin_rpc {
    use blocks::{header::json::BlockHeaderJson, tipset_json::TipsetJson};
    use cid::json::CidJson;
    use jsonrpc_v2::Error as JsonRpcError;
    use message::unsigned_message::json::UnsignedMessageJson;

    use crate::{call, call_method, call_params};

    pub async fn auth_new(perm: Vec<String>) -> Result<String, JsonRpcError> {
        call_params("Filecoin.AuthNew", perm).await
    }

    pub async fn chain_get_block(cid: CidJson) -> Result<BlockHeaderJson, JsonRpcError> {
        call_params("Filecoin.ChainGetBlock", serde_json::to_string(&cid)?).await
    }

    pub async fn chain_get_genesis() -> Result<TipsetJson, JsonRpcError> {
        call_method("Filecoin.ChainGetGenesis").await
    }

    pub async fn chain_get_head() -> Result<TipsetJson, JsonRpcError> {
        call_method("Filecoin.ChainHead").await
    }

    pub async fn chain_get_messages(cid: CidJson) -> Result<UnsignedMessageJson, JsonRpcError> {
        call_params("Filecoin.ChainGetMessage", serde_json::to_string(&cid)?).await
    }

    pub async fn chain_read_obj(cid: CidJson) -> Result<Vec<u8>, JsonRpcError> {
        call_params("Filecoin.ChainGetObj", serde_json::to_string(&cid)?).await
    }
}
