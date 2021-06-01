// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::{Error, Id, RequestObject, V2};
use log::{debug, error};
use parity_multiaddr::{Multiaddr, Protocol};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::env;

const DEFAULT_MULTIADDRESS: &str = "/ip4/127.0.0.1/tcp/1234/http";
const DEFAULT_URL: &str = "http://127.0.0.1:1234/rpc/v0";
const DEFAULT_PROTOCOL: &str = "http";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: &str = "1234";
const API_INFO_KEY: &str = "FULLNODE_API_INFO";
const RPC_ENDPOINT: &str = "rpc/v0";

/// Error object in a response
#[derive(Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResponse<R> {
    Result {
        jsonrpc: V2,
        result: R,
        id: Id,
    },
    Error {
        jsonrpc: V2,
        error: JsonRpcError,
        id: Id,
    },
}

struct URL {
    protocol: String,
    port: String,
    host: String,
}

/// Parses a multiaddress into a URL
fn multiaddress_to_url(ma_str: String) -> String {
    // Parse Multiaddress string
    let ma: Multiaddr = ma_str.parse().expect("Parse multiaddress");

    // Fold Multiaddress into a URL struct
    let addr = ma.into_iter().fold(
        URL {
            protocol: DEFAULT_PROTOCOL.to_owned(),
            port: DEFAULT_PORT.to_owned(),
            host: DEFAULT_HOST.to_owned(),
        },
        |mut addr, protocol| {
            match protocol {
                Protocol::Ip6(ip) => {
                    addr.host = ip.to_string();
                }
                Protocol::Ip4(ip) => {
                    addr.host = ip.to_string();
                }
                Protocol::Dns(dns) => {
                    addr.host = dns.to_string();
                }
                Protocol::Dns4(dns) => {
                    addr.host = dns.to_string();
                }
                Protocol::Dns6(dns) => {
                    addr.host = dns.to_string();
                }
                Protocol::Dnsaddr(dns) => {
                    addr.host = dns.to_string();
                }
                Protocol::Tcp(p) => {
                    addr.port = p.to_string();
                }
                Protocol::Http => {
                    addr.protocol = "http".to_string();
                }
                Protocol::Https => {
                    addr.protocol = "https".to_string();
                }
                _ => {}
            };
            addr
        },
    );

    // Format, print and return the URL
    let url = format!(
        "{}://{}:{}/{}",
        addr.protocol, addr.host, addr.port, RPC_ENDPOINT
    );
    debug!("Using JSON-RPC v2 HTTP URL: {}", url);
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
    if api_info.matches(':').count() > 1 {
        return Err(jsonrpc_v2::Error::from(format!(
            "Improperly formatted multiaddress value provided for the {} environment variable. Value was: {}",
            API_INFO_KEY, api_info,
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

    match rpc_res {
        JsonRpcResponse::Result { result, .. } => Ok(result),
        JsonRpcResponse::Error { error, .. } => {
            return Err(error.message.into());
        }
    }
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
    P: Serialize,
    R: DeserializeOwned,
{
    let rpc_req = jsonrpc_v2::RequestObject::request()
        .with_method(method_name)
        .with_params(serde_json::to_value(vec![params])?)
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

    pub async fn auth_new(perm: Vec<String>) -> Result<Vec<u8>, Error> {
        call_params("Filecoin.AuthNew", perm).await
    }

    pub async fn chain_get_block(cid: CidJson) -> Result<BlockHeaderJson, Error> {
        call_params("Filecoin.ChainGetBlock", cid).await
    }

    pub async fn chain_get_genesis() -> Result<TipsetJson, Error> {
        call_method("Filecoin.ChainGetGenesis").await
    }

    pub async fn chain_get_head() -> Result<TipsetJson, Error> {
        call_method("Filecoin.ChainHead").await
    }

    pub async fn chain_get_messages(cid: CidJson) -> Result<UnsignedMessageJson, Error> {
        call_params("Filecoin.ChainGetMessage", cid).await
    }

    pub async fn chain_read_obj(cid: CidJson) -> Result<Vec<u8>, Error> {
        call_params("Filecoin.ChainReadObj", cid).await
    }
}
