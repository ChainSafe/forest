// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::{verify_token, JWT_IDENTIFIER};
use crate::key_management::KeyStore;
use crate::rpc::{
    auth, beacon, chain, common, eth, gas, mpool, net, node, state, sync, wallet, RpcMethod as _,
    CANCEL_METHOD_NAME,
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
    access.insert(auth::AuthNew::NAME, Access::Admin);
    access.insert(auth::AuthVerify::NAME, Access::Read);

    // Beacon API
    access.insert(beacon::BeaconGetEntry::NAME, Access::Read);

    // Chain API
    access.insert(chain::ChainGetMessage::NAME, Access::Read);
    access.insert(chain::ChainExport::NAME, Access::Read);
    access.insert(chain::ChainReadObj::NAME, Access::Read);
    access.insert(chain::ChainGetPath::NAME, Access::Read);
    access.insert(chain::ChainHasObj::NAME, Access::Read);
    access.insert(chain::ChainGetBlockMessages::NAME, Access::Read);
    access.insert(chain::ChainGetTipSetByHeight::NAME, Access::Read);
    access.insert(chain::ChainGetTipSetAfterHeight::NAME, Access::Read);
    access.insert(chain::ChainGetGenesis::NAME, Access::Read);
    access.insert(chain::ChainHead::NAME, Access::Read);
    access.insert(chain::ChainGetBlock::NAME, Access::Read);
    access.insert(chain::ChainGetTipSet::NAME, Access::Read);
    access.insert(chain::ChainSetHead::NAME, Access::Admin);
    access.insert(chain::ChainGetMinBaseFee::NAME, Access::Admin);
    access.insert(chain::ChainTipSetWeight::NAME, Access::Read);
    access.insert(chain::ChainGetMessagesInTipset::NAME, Access::Read);
    access.insert(chain::ChainGetParentMessages::NAME, Access::Read);
    access.insert(chain::CHAIN_NOTIFY, Access::Read);
    access.insert(chain::ChainGetParentReceipts::NAME, Access::Read);

    // Message Pool API
    access.insert(mpool::MpoolGetNonce::NAME, Access::Read);
    access.insert(mpool::MpoolPending::NAME, Access::Read);
    access.insert(mpool::MpoolSelect::NAME, Access::Read);
    // Lotus limits `MPOOL_PUSH`` to `Access::Write`. However, since messages
    // can always be pushed over the p2p protocol, limiting the RPC doesn't
    // improve security.
    access.insert(mpool::MpoolPush::NAME, Access::Read);
    access.insert(mpool::MpoolPushMessage::NAME, Access::Sign);

    // Sync API
    access.insert(sync::SyncCheckBad::NAME, Access::Read);
    access.insert(sync::SyncMarkBad::NAME, Access::Admin);
    access.insert(sync::SyncState::NAME, Access::Read);

    // Wallet API
    access.insert(wallet::WalletBalance::NAME, Access::Read);
    access.insert(wallet::WalletDefaultAddress::NAME, Access::Read);
    access.insert(wallet::WalletExport::NAME, Access::Admin);
    access.insert(wallet::WalletHas::NAME, Access::Write);
    access.insert(wallet::WalletImport::NAME, Access::Admin);
    access.insert(wallet::WalletList::NAME, Access::Write);
    access.insert(wallet::WalletNew::NAME, Access::Write);
    access.insert(wallet::WalletSetDefault::NAME, Access::Write);
    access.insert(wallet::WalletSign::NAME, Access::Sign);
    access.insert(wallet::WalletValidateAddress::NAME, Access::Read);
    access.insert(wallet::WalletVerify::NAME, Access::Read);
    access.insert(wallet::WalletDelete::NAME, Access::Write);

    // State API
    access.insert(state::STATE_CALL, Access::Read);
    access.insert(state::STATE_REPLAY, Access::Read);
    access.insert(state::STATE_GET_ACTOR, Access::Read);
    access.insert(state::STATE_MARKET_BALANCE, Access::Read);
    access.insert(state::STATE_MARKET_DEALS, Access::Read);
    access.insert(state::STATE_MINER_INFO, Access::Read);
    access.insert(state::MINER_GET_BASE_INFO, Access::Read);
    access.insert(state::STATE_MINER_ACTIVE_SECTORS, Access::Read);
    access.insert(state::STATE_MINER_FAULTS, Access::Read);
    access.insert(state::STATE_MINER_RECOVERIES, Access::Read);
    access.insert(state::STATE_MINER_POWER, Access::Read);
    access.insert(state::STATE_MINER_DEADLINES, Access::Read);
    access.insert(state::STATE_MINER_PROVING_DEADLINE, Access::Read);
    access.insert(state::STATE_MINER_AVAILABLE_BALANCE, Access::Read);
    access.insert(state::STATE_GET_RECEIPT, Access::Read);
    access.insert(state::STATE_WAIT_MSG, Access::Read);
    access.insert(state::STATE_SEARCH_MSG, Access::Read);
    access.insert(state::STATE_SEARCH_MSG_LIMITED, Access::Read);
    access.insert(state::STATE_NETWORK_NAME, Access::Read);
    access.insert(state::STATE_NETWORK_VERSION, Access::Read);
    access.insert(state::STATE_ACCOUNT_KEY, Access::Read);
    access.insert(state::STATE_LOOKUP_ID, Access::Read);
    access.insert(state::STATE_FETCH_ROOT, Access::Read);
    access.insert(state::STATE_GET_RANDOMNESS_FROM_TICKETS, Access::Read);
    access.insert(state::STATE_GET_RANDOMNESS_FROM_BEACON, Access::Read);
    access.insert(state::STATE_READ_STATE, Access::Read);
    access.insert(state::STATE_CIRCULATING_SUPPLY, Access::Read);
    access.insert(state::StateSectorGetInfo::NAME, Access::Read);
    access.insert(state::STATE_LIST_MESSAGES, Access::Read);
    access.insert(state::STATE_LIST_MINERS, Access::Read);
    access.insert(state::STATE_MINER_SECTOR_COUNT, Access::Read);
    access.insert(state::STATE_MINER_SECTORS, Access::Read);
    access.insert(state::STATE_MINER_PARTITIONS, Access::Read);
    access.insert(state::STATE_VERIFIED_CLIENT_STATUS, Access::Read);
    access.insert(state::STATE_MARKET_STORAGE_DEAL, Access::Read);
    access.insert(state::STATE_VM_CIRCULATING_SUPPLY_INTERNAL, Access::Read);
    access.insert(state::MSIG_GET_AVAILABLE_BALANCE, Access::Read);
    access.insert(state::MSIG_GET_PENDING, Access::Read);
    access.insert(state::STATE_DEAL_PROVIDER_COLLATERAL_BOUNDS, Access::Read);
    access.insert(state::StateGetBeaconEntry::NAME, Access::Read);
    access.insert(state::StateSectorPreCommitInfo::NAME, Access::Read);

    // Gas API
    access.insert(gas::GAS_ESTIMATE_GAS_LIMIT, Access::Read);
    access.insert(gas::GAS_ESTIMATE_GAS_PREMIUM, Access::Read);
    access.insert(gas::GAS_ESTIMATE_FEE_CAP, Access::Read);
    access.insert(gas::GAS_ESTIMATE_MESSAGE_GAS, Access::Read);

    // Common API
    access.insert(common::Version::NAME, Access::Read);
    access.insert(common::Session::NAME, Access::Read);
    access.insert(common::Shutdown::NAME, Access::Admin);
    access.insert(common::StartTime::NAME, Access::Read);

    // Net API
    access.insert(net::NetAddrsListen::NAME, Access::Read);
    access.insert(net::NetPeers::NAME, Access::Read);
    access.insert(net::NetListening::NAME, Access::Read);
    access.insert(net::NetInfo::NAME, Access::Read);
    access.insert(net::NetConnect::NAME, Access::Write);
    access.insert(net::NetDisconnect::NAME, Access::Write);
    access.insert(net::NetAgentVersion::NAME, Access::Read);
    access.insert(net::NetAutoNatStatus::NAME, Access::Read);
    access.insert(net::NetVersion::NAME, Access::Read);

    // Node API
    access.insert(node::NodeStatus::NAME, Access::Read);

    // Eth API
    access.insert(eth::ETH_ACCOUNTS, Access::Read);
    access.insert(eth::ETH_BLOCK_NUMBER, Access::Read);
    access.insert(eth::ETH_CHAIN_ID, Access::Read);
    access.insert(eth::ETH_GAS_PRICE, Access::Read);
    access.insert(eth::ETH_GET_BALANCE, Access::Read);
    access.insert(eth::ETH_SYNCING, Access::Read);
    access.insert(eth::ETH_GET_BLOCK_BY_NUMBER, Access::Read);
    access.insert(eth::WEB3_CLIENT_VERSION, Access::Read);

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
