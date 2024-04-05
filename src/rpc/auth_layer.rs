// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::RpcMethod as _;
use crate::auth::{verify_token, JWT_IDENTIFIER};
use crate::key_management::KeyStore;
use crate::rpc::{
    auth_api, beacon_api, chain_api, common_api, eth_api, gas_api, mpool_api, net_api, node_api,
    state_api, sync_api, wallet_api, CANCEL_METHOD_NAME,
};
use ahash::{HashMap, HashMapExt as _};
use futures::future::BoxFuture;
use futures::FutureExt;
use hyper::header::{HeaderValue, AUTHORIZATION};
use hyper::HeaderMap;
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::{error::ErrorCode, ErrorObject};
use jsonrpsee::MethodResponse;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::Layer;
use tracing::debug;

/// Access levels to be checked against JWT claims
enum Access {
    Admin,
    Sign,
    Write,
    Read,
}

/// Access mapping between method names and access levels
/// Checked against JWT claims on every request
static ACCESS_MAP: Lazy<HashMap<&str, Access>> = Lazy::new(|| {
    let mut access = HashMap::new();

    // Auth API
    access.insert(auth_api::AUTH_NEW, Access::Admin);
    access.insert(auth_api::AUTH_VERIFY, Access::Read);

    // Beacon API
    access.insert(beacon_api::BEACON_GET_ENTRY, Access::Read);

    // Chain API
    access.insert(chain_api::CHAIN_GET_MESSAGE, Access::Read);
    access.insert(chain_api::CHAIN_EXPORT, Access::Read);
    access.insert(chain_api::CHAIN_READ_OBJ, Access::Read);
    access.insert(chain_api::CHAIN_GET_PATH, Access::Read);
    access.insert(chain_api::CHAIN_HAS_OBJ, Access::Read);
    access.insert(chain_api::CHAIN_GET_BLOCK_MESSAGES, Access::Read);
    access.insert(chain_api::CHAIN_GET_TIPSET_BY_HEIGHT, Access::Read);
    access.insert(chain_api::CHAIN_GET_TIPSET_AFTER_HEIGHT, Access::Read);
    access.insert(chain_api::CHAIN_GET_GENESIS, Access::Read);
    access.insert(chain_api::CHAIN_HEAD, Access::Read);
    access.insert(chain_api::CHAIN_GET_BLOCK, Access::Read);
    access.insert(chain_api::CHAIN_GET_TIPSET, Access::Read);
    access.insert(chain_api::CHAIN_SET_HEAD, Access::Admin);
    access.insert(chain_api::CHAIN_GET_MIN_BASE_FEE, Access::Admin);
    access.insert(chain_api::CHAIN_GET_MESSAGES_IN_TIPSET, Access::Read);
    access.insert(chain_api::CHAIN_GET_PARENT_MESSAGES, Access::Read);
    access.insert(chain_api::CHAIN_NOTIFY, Access::Read);
    access.insert(chain_api::CHAIN_GET_PARENT_RECEIPTS, Access::Read);

    // Message Pool API
    access.insert(mpool_api::MpoolGetNonce::NAME, Access::Read);
    access.insert(mpool_api::MpoolPending::NAME, Access::Read);
    // Lotus limits `MPOOL_PUSH`` to `Access::Write`. However, since messages
    // can always be pushed over the p2p protocol, limiting the RPC doesn't
    // improve security.
    access.insert(mpool_api::MpoolPush::NAME, Access::Read);
    access.insert(mpool_api::MpoolPushMessage::NAME, Access::Sign);

    // Sync API
    access.insert(sync_api::SYNC_CHECK_BAD, Access::Read);
    access.insert(sync_api::SYNC_MARK_BAD, Access::Admin);
    access.insert(sync_api::SYNC_STATE, Access::Read);

    // Wallet API
    access.insert(wallet_api::WALLET_BALANCE, Access::Write);
    access.insert(wallet_api::WALLET_BALANCE, Access::Read);
    access.insert(wallet_api::WALLET_DEFAULT_ADDRESS, Access::Read);
    access.insert(wallet_api::WALLET_EXPORT, Access::Admin);
    access.insert(wallet_api::WALLET_HAS, Access::Write);
    access.insert(wallet_api::WALLET_IMPORT, Access::Admin);
    access.insert(wallet_api::WALLET_LIST, Access::Write);
    access.insert(wallet_api::WALLET_NEW, Access::Write);
    access.insert(wallet_api::WALLET_SET_DEFAULT, Access::Write);
    access.insert(wallet_api::WALLET_SIGN, Access::Sign);
    access.insert(wallet_api::WALLET_VALIDATE_ADDRESS, Access::Read);
    access.insert(wallet_api::WALLET_VERIFY, Access::Read);
    access.insert(wallet_api::WALLET_DELETE, Access::Write);

    // State API
    access.insert(state_api::STATE_CALL, Access::Read);
    access.insert(state_api::STATE_REPLAY, Access::Read);
    access.insert(state_api::STATE_GET_ACTOR, Access::Read);
    access.insert(state_api::STATE_MARKET_BALANCE, Access::Read);
    access.insert(state_api::STATE_MARKET_DEALS, Access::Read);
    access.insert(state_api::STATE_MINER_INFO, Access::Read);
    access.insert(state_api::MINER_GET_BASE_INFO, Access::Read);
    access.insert(state_api::STATE_MINER_ACTIVE_SECTORS, Access::Read);
    access.insert(state_api::STATE_MINER_FAULTS, Access::Read);
    access.insert(state_api::STATE_MINER_RECOVERIES, Access::Read);
    access.insert(state_api::STATE_MINER_POWER, Access::Read);
    access.insert(state_api::STATE_MINER_DEADLINES, Access::Read);
    access.insert(state_api::STATE_MINER_PROVING_DEADLINE, Access::Read);
    access.insert(state_api::STATE_MINER_AVAILABLE_BALANCE, Access::Read);
    access.insert(state_api::STATE_GET_RECEIPT, Access::Read);
    access.insert(state_api::STATE_WAIT_MSG, Access::Read);
    access.insert(state_api::STATE_SEARCH_MSG, Access::Read);
    access.insert(state_api::STATE_SEARCH_MSG_LIMITED, Access::Read);
    access.insert(state_api::STATE_NETWORK_NAME, Access::Read);
    access.insert(state_api::STATE_NETWORK_VERSION, Access::Read);
    access.insert(state_api::STATE_ACCOUNT_KEY, Access::Read);
    access.insert(state_api::STATE_LOOKUP_ID, Access::Read);
    access.insert(state_api::STATE_FETCH_ROOT, Access::Read);
    access.insert(state_api::STATE_GET_RANDOMNESS_FROM_TICKETS, Access::Read);
    access.insert(state_api::STATE_GET_RANDOMNESS_FROM_BEACON, Access::Read);
    access.insert(state_api::STATE_READ_STATE, Access::Read);
    access.insert(state_api::STATE_CIRCULATING_SUPPLY, Access::Read);
    access.insert(state_api::STATE_SECTOR_GET_INFO, Access::Read);
    access.insert(state_api::STATE_LIST_MESSAGES, Access::Read);
    access.insert(state_api::STATE_LIST_MINERS, Access::Read);
    access.insert(state_api::STATE_MINER_SECTOR_COUNT, Access::Read);
    access.insert(state_api::STATE_VERIFIED_CLIENT_STATUS, Access::Read);
    access.insert(state_api::STATE_MARKET_STORAGE_DEAL, Access::Read);
    access.insert(
        state_api::STATE_VM_CIRCULATING_SUPPLY_INTERNAL,
        Access::Read,
    );
    access.insert(state_api::MSIG_GET_AVAILABLE_BALANCE, Access::Read);
    access.insert(state_api::MSIG_GET_PENDING, Access::Read);

    // Gas API
    access.insert(gas_api::GAS_ESTIMATE_GAS_LIMIT, Access::Read);
    access.insert(gas_api::GAS_ESTIMATE_GAS_PREMIUM, Access::Read);
    access.insert(gas_api::GAS_ESTIMATE_FEE_CAP, Access::Read);
    access.insert(gas_api::GAS_ESTIMATE_MESSAGE_GAS, Access::Read);

    // Common API
    access.insert(common_api::VERSION, Access::Read);
    access.insert(common_api::SESSION, Access::Read);
    access.insert(common_api::SHUTDOWN, Access::Admin);
    access.insert(common_api::START_TIME, Access::Read);

    // Net API
    access.insert(net_api::NET_ADDRS_LISTEN, Access::Read);
    access.insert(net_api::NET_PEERS, Access::Read);
    access.insert(net_api::NET_LISTENING, Access::Read);
    access.insert(net_api::NET_INFO, Access::Read);
    access.insert(net_api::NET_CONNECT, Access::Write);
    access.insert(net_api::NET_DISCONNECT, Access::Write);
    access.insert(net_api::NET_AGENT_VERSION, Access::Read);
    access.insert(net_api::NET_AUTO_NAT_STATUS, Access::Read);
    access.insert(net_api::NET_VERSION, Access::Read);

    // Node API
    access.insert(node_api::NODE_STATUS, Access::Read);

    // Eth API
    access.insert(eth_api::ETH_ACCOUNTS, Access::Read);
    access.insert(eth_api::ETH_BLOCK_NUMBER, Access::Read);
    access.insert(eth_api::ETH_CHAIN_ID, Access::Read);
    access.insert(eth_api::ETH_GAS_PRICE, Access::Read);
    access.insert(eth_api::ETH_GET_BALANCE, Access::Read);
    access.insert(eth_api::ETH_SYNCING, Access::Read);

    // Pubsub API
    access.insert(CANCEL_METHOD_NAME, Access::Read);

    access
});

/// Checks an access enumeration against provided JWT claims
fn check_access(access: &Access, claims: &[String]) -> bool {
    match access {
        Access::Admin => claims.contains(&"admin".to_owned()),
        Access::Sign => claims.contains(&"sign".to_owned()),
        Access::Write => claims.contains(&"write".to_owned()),
        Access::Read => claims.contains(&"read".to_owned()),
    }
}

#[derive(Clone)]
pub struct AuthLayer {
    pub headers: HeaderMap,
    pub keystore: Arc<RwLock<KeyStore>>,
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        AuthMiddleware {
            headers: self.headers.clone(),
            keystore: self.keystore.clone(),
            service,
        }
    }
}

#[derive(Clone)]
pub struct AuthMiddleware<S> {
    headers: HeaderMap,
    keystore: Arc<RwLock<KeyStore>>,
    service: S,
}

impl<'a, S> RpcServiceT<'a> for AuthMiddleware<S>
where
    S: RpcServiceT<'a> + Send + Sync + Clone + 'static,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, req: jsonrpsee::types::Request<'a>) -> Self::Future {
        let headers = self.headers.clone();
        let keystore = self.keystore.clone();
        let service = self.service.clone();

        async move {
            let auth_header = headers.get(AUTHORIZATION).cloned();
            let res = check_permissions(keystore, auth_header, req.method_name()).await;

            match res {
                Ok(()) => service.call(req).await,
                Err(code) => MethodResponse::error(req.id(), ErrorObject::from(code)),
            }
        }
        .boxed()
    }
}

/// Verify JWT Token and return the token's permissions.
async fn auth_verify(token: &str, keystore: Arc<RwLock<KeyStore>>) -> anyhow::Result<Vec<String>> {
    let ks = keystore.read().await;
    let ki = ks.get(JWT_IDENTIFIER)?;
    let perms = verify_token(token, ki.private_key())?;
    Ok(perms)
}

async fn check_permissions(
    keystore: Arc<RwLock<KeyStore>>,
    auth_header: Option<HeaderValue>,
    method: &str,
) -> anyhow::Result<(), ErrorCode> {
    let claims = match auth_header {
        Some(token) => {
            let token = token.to_str().map_err(|_| ErrorCode::ParseError)?;

            debug!("JWT from HTTP Header: {}", token);

            auth_verify(token, keystore)
                .await
                .map_err(|_| ErrorCode::InvalidRequest)?
        }
        // If no token is passed, assume read behavior
        None => vec!["read".to_owned()],
    };
    debug!("Decoded JWT Claims: {}", claims.join(","));

    match ACCESS_MAP.get(&method) {
        Some(access) => {
            if check_access(access, &claims) {
                Ok(())
            } else {
                Err(ErrorCode::InvalidRequest)
            }
        }
        None => Err(ErrorCode::MethodNotFound),
    }
}
