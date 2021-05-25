// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::{Error, Id, RequestObject};
use log::{error, info};
use regex::Regex;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use std::{env, fmt};

const DEFAULT_MULTIADDRESS: &str = "/ip4/127.0.0.1/tcp/1234/http";
const DEFAULT_URL: &str = "http://127.0.0.1:1234/rpc/v0";
const API_INFO_KEY: &str = "FULLNODE_API_INFO";
const RPC_ENDPOINT: &str = "rpc/v0";

/// Error object in a response
#[derive(Deserialize)]
struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

impl fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error ({}): {}", self.code, self.message)
    }
}

#[derive(Deserialize)]
struct JsonRpcResponse<T> {
    pub jsonrpc: String,
    pub result: Option<T>,
    pub error: Option<JsonRpcError>,
    pub id: Option<Id>,
}

/// Parses an ip4 multiaddress into an HTTP URL
fn multiaddress_to_url(ma_str: String) -> String {
    // Example haystack: "/ip4/127.0.0.1/tcp/1234/http"
    let regex = Regex::new(r"/ip4/(?P<protocol>.*)/tcp/(?P<host>.*)/(?P<port>.*)").unwrap();

    // Parse multiaddress using regex named captures.
    // If the regex cannot match, log an error and return the default URL.
    let url = match regex.captures(&ma_str) {
        Some(segments) => {
            let protocol = segments.name("protocol").unwrap().as_str();
            let host = segments.name("host").unwrap().as_str();
            let port = segments.name("port").unwrap().as_str();
            let path = RPC_ENDPOINT;
            format!(
                "{protocol}://{host}:{port}/{path}",
                protocol = protocol,
                host = host,
                port = port,
                path = path
            )
        }
        None => {
            error!(
                "Parse Error: {} could not be parsed as a ip4 multiaddress",
                ma_str
            );
            DEFAULT_URL.to_owned()
        }
    };

    // Print and return the URL
    info!("Using JSON-RPC v2 HTTP URL: {}", url);
    url
}

/// Utility method for sending RPC requests over HTTP
async fn call<R>(rpc_call: RequestObject) -> Result<R, Error>
where
    R: DeserializeOwned,
{
    // Get API INFO environment variable if exists, otherwise, use default multiaddress
    let api_info = env::var(API_INFO_KEY).unwrap_or_else(|_| DEFAULT_MULTIADDRESS.to_owned());

    // Input sanity checks
    if !api_info.starts_with("/ip4/") {
        return Err(jsonrpc_v2::Error::from(format!(
            "Only IPv4 addresses are currently supported values for the {} environment variable",
            API_INFO_KEY,
        )));
    }

    if api_info.split(':').count() > 1 {
        return Err(jsonrpc_v2::Error::from(format!(
            "Improperly formatted multiaddress value provided for the {} environment variable",
            API_INFO_KEY,
        )));
    }

    // Split the JWT off if present, format multiaddress as URL, then post RPC request to URL
    let mut http_res = match &api_info.split_once(':') {
        Some((jwt, host)) => surf::post(multiaddress_to_url(host.to_string()))
            .body(surf::Body::from_json(&rpc_call)?)
            .content_type("application/json-rpc")
            .header("Authorization", jwt.to_string()),
        None => surf::post(DEFAULT_URL).body(surf::Body::from_json(&rpc_call)?),
    }
    .await?;

    let res = http_res.body_string().await?;

    // Return the parsed RPC result
    let rpc_res: JsonRpcResponse<R> = match serde_json::from_str(&res) {
        Ok(r) => r,
        Err(e) => {
            let err = format!(
                "Parse Error: Response from RPC endpoint could not be parsed. Error was: {}",
                e
            );
            error!("{}", &err);
            return Err(err.into());
        }
    };

    if let Some(why) = rpc_res.error {
        return Err(why.message.into());
    }

    rpc_res.result.ok_or_else(|| {
        "Unknown Error: Server responded with neither a response nor an error".into()
    })
}

/// Call an RPC method without params
pub async fn call_method<R>(method_name: &str) -> Result<R, Error>
where
    R: DeserializeOwned,
{
    let rpc_req = jsonrpc_v2::RequestObject::request()
        .with_method(method_name)
        .finish();

    call(rpc_req).await.map_err(|e| e)
}

/// Call an RPC method with params
pub async fn call_params<P, R>(method_name: &str, params: P) -> Result<R, Error>
where
    P: Into<Value>,
    R: DeserializeOwned,
{
    let rpc_req = jsonrpc_v2::RequestObject::request()
        .with_method(method_name)
        .with_params(params)
        .finish();

    call(rpc_req).await.map_err(|e| e)
}

/// Filecoin RPC client interface methods
pub mod filecoin_rpc {
    use blocks::{header::json::BlockHeaderJson, tipset_json::TipsetJson};
    use cid::json::CidJson;
    use jsonrpc_v2::Error;
    use message::unsigned_message::json::UnsignedMessageJson;

    use crate::{call_method, call_params};

    pub async fn auth_new(perm: Vec<String>) -> Result<String, Error> {
        call_params("Filecoin.AuthNew", perm).await
    }

    pub async fn chain_get_block(cid: CidJson) -> Result<BlockHeaderJson, Error> {
        call_params("Filecoin.ChainGetBlock", serde_json::to_string(&cid)?).await
    }

    pub async fn chain_get_genesis() -> Result<TipsetJson, Error> {
        call_method("Filecoin.ChainGetGenesis").await
    }

    pub async fn chain_get_head() -> Result<TipsetJson, Error> {
        call_method("Filecoin.ChainHead").await
    }

    pub async fn chain_get_messages(cid: CidJson) -> Result<UnsignedMessageJson, Error> {
        call_params("Filecoin.ChainGetMessage", serde_json::to_string(&cid)?).await
    }

    pub async fn chain_read_obj(cid: CidJson) -> Result<Vec<u8>, Error> {
        call_params("Filecoin.ChainGetObj", serde_json::to_string(&cid)?).await
    }
}
