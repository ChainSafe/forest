// Copyright 2019-2022 ChainSafe Systems
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
            .content_type("application/json-rpc")
            .body(surf::Body::from_json(&rpc_call)?)
            .header("Authorization", jwt.to_string()),
        None => surf::post(DEFAULT_URL)
            .content_type("application/json-rpc")
            .body(surf::Body::from_json(&rpc_call)?),
    }
    .await?;

    let res = http_res.body_string().await?;
    let code = http_res.status() as i64;

    if code != 200 {
        return Err(jsonrpc_v2::Error::Full {
            message: format!("Error code from HTTP Response: {}", code),
            code,
            data: None,
        });
    }

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
        JsonRpcResponse::Error { error, .. } => Err(error.message.into()),
    }
}

/// Call an RPC method with params
pub async fn call_params<P, R>(method_name: &str, params: P) -> Result<R, Error>
where
    P: Serialize,
    R: DeserializeOwned,
{
    let rpc_req = jsonrpc_v2::RequestObject::request()
        .with_method(method_name)
        .with_params(serde_json::to_value(params)?)
        .finish();

    call(rpc_req).await.map_err(|e| e)
}

/// Filecoin RPC client interface methods
pub mod filecoin_rpc {
    use jsonrpc_v2::Error;

    use crate::call_params;
    use rpc_api::{auth_api::*, chain_api::*, sync_api::*, wallet_api::*};

    /// Auth
    pub async fn auth_new(perm: AuthNewParams) -> Result<AuthNewResult, Error> {
        call_params(AUTH_NEW, perm).await
    }

    pub async fn chain_get_block(cid: ChainGetBlockParams) -> Result<ChainGetBlockResult, Error> {
        call_params(CHAIN_GET_BLOCK, cid).await
    }

    pub async fn chain_get_genesis() -> Result<ChainGetGenesisResult, Error> {
        call_params(CHAIN_GET_GENESIS, ()).await
    }

    pub async fn chain_head() -> Result<ChainHeadResult, Error> {
        call_params(CHAIN_HEAD, ()).await
    }

    pub async fn chain_get_message(
        cid: ChainGetMessageParams,
    ) -> Result<ChainGetMessageResult, Error> {
        call_params(CHAIN_GET_MESSAGE, cid).await
    }

    pub async fn chain_read_obj(cid: ChainReadObjParams) -> Result<ChainReadObjResult, Error> {
        call_params(CHAIN_READ_OBJ, cid).await
    }

    /// Wallet
    pub async fn wallet_new(signature_type: WalletNewParams) -> Result<WalletNewResult, Error> {
        call_params(WALLET_NEW, signature_type).await
    }

    pub async fn wallet_default_address() -> Result<WalletDefaultAddressResult, Error> {
        call_params(WALLET_DEFAULT_ADDRESS, ()).await
    }

    pub async fn wallet_balance(
        address: WalletBalanceParams,
    ) -> Result<WalletBalanceResult, Error> {
        call_params(WALLET_BALANCE, address).await
    }

    pub async fn wallet_export(address: WalletExportParams) -> Result<WalletExportResult, Error> {
        call_params(WALLET_EXPORT, address).await
    }

    pub async fn wallet_import(key: WalletImportParams) -> Result<WalletImportResult, Error> {
        call_params(WALLET_IMPORT, key).await
    }

    pub async fn wallet_list() -> Result<WalletListResult, Error> {
        call_params(WALLET_LIST, ()).await
    }

    pub async fn wallet_has(key: WalletHasParams) -> Result<WalletHasResult, Error> {
        call_params(WALLET_HAS, key).await
    }

    pub async fn wallet_set_default(
        address: WalletSetDefaultParams,
    ) -> Result<WalletSetDefaultResult, Error> {
        call_params(WALLET_SET_DEFAULT, address).await
    }

    pub async fn wallet_sign(message: WalletSignParams) -> Result<WalletSignResult, Error> {
        call_params(WALLET_SIGN, message).await
    }

    pub async fn wallet_verify(message: WalletVerifyParams) -> Result<WalletVerifyResult, Error> {
        call_params(WALLET_VERIFY, message).await
    }

    /// Chain-Sync
    pub async fn sync_check_bad(params: SyncCheckBadParams) -> Result<SyncCheckBadResult, Error> {
        call_params(SYNC_CHECK_BAD, params).await
    }

    pub async fn sync_mark_bad(params: SyncMarkBadParams) -> Result<SyncMarkBadResult, Error> {
        call_params(SYNC_MARK_BAD, params).await
    }

    pub async fn sync_state(params: SyncStateParams) -> Result<SyncStateResult, Error> {
        call_params(SYNC_STATE, params).await
    }
}
