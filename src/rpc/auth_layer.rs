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
    access.insert(sync::SyncSubmitBlock::NAME, Access::Write);

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
    access.insert(state::MinerGetBaseInfo::NAME, Access::Read);
    access.insert(state::StateCall::NAME, Access::Read);
    access.insert(state::StateNetworkName::NAME, Access::Read);
    access.insert(state::StateReplay::NAME, Access::Read);
    access.insert(state::StateGetActor::NAME, Access::Read);
    access.insert(state::StateMarketBalance::NAME, Access::Read);
    access.insert(state::StateMarketDeals::NAME, Access::Read);
    access.insert(state::StateMinerInfo::NAME, Access::Read);
    access.insert(state::StateMinerActiveSectors::NAME, Access::Read);
    access.insert(state::StateMinerFaults::NAME, Access::Read);
    access.insert(state::StateMinerRecoveries::NAME, Access::Read);
    access.insert(state::StateMinerPower::NAME, Access::Read);
    access.insert(state::StateMinerDeadlines::NAME, Access::Read);
    access.insert(state::StateMinerProvingDeadline::NAME, Access::Read);
    access.insert(state::StateMinerAvailableBalance::NAME, Access::Read);
    access.insert(state::StateMinerInitialPledgeCollateral::NAME, Access::Read);
    access.insert(state::StateGetReceipt::NAME, Access::Read);
    access.insert(state::StateWaitMsg::NAME, Access::Read);
    access.insert(state::StateSearchMsg::NAME, Access::Read);
    access.insert(state::StateSearchMsgLimited::NAME, Access::Read);
    access.insert(state::StateNetworkVersion::NAME, Access::Read);
    access.insert(state::StateAccountKey::NAME, Access::Read);
    access.insert(state::StateLookupID::NAME, Access::Read);
    access.insert(state::StateFetchRoot::NAME, Access::Read);
    access.insert(state::StateGetRandomnessFromTickets::NAME, Access::Read);
    access.insert(state::StateGetRandomnessFromBeacon::NAME, Access::Read);
    access.insert(state::StateReadState::NAME, Access::Read);
    access.insert(state::StateCirculatingSupply::NAME, Access::Read);
    access.insert(state::StateSectorGetInfo::NAME, Access::Read);
    access.insert(state::StateListMessages::NAME, Access::Read);
    access.insert(state::StateListMiners::NAME, Access::Read);
    access.insert(state::StateMinerSectorCount::NAME, Access::Read);
    access.insert(state::StateMinerSectors::NAME, Access::Read);
    access.insert(state::StateMinerPartitions::NAME, Access::Read);
    access.insert(state::StateVerifiedClientStatus::NAME, Access::Read);
    access.insert(state::StateMarketStorageDeal::NAME, Access::Read);
    access.insert(state::StateVMCirculatingSupplyInternal::NAME, Access::Read);
    access.insert(state::MsigGetAvailableBalance::NAME, Access::Read);
    access.insert(state::MsigGetPending::NAME, Access::Read);
    access.insert(state::StateDealProviderCollateralBounds::NAME, Access::Read);
    access.insert(state::StateGetBeaconEntry::NAME, Access::Read);
    access.insert(state::StateSectorPreCommitInfo::NAME, Access::Read);

    // Gas API
    access.insert(gas::GasEstimateGasLimit::NAME, Access::Read);
    access.insert(gas::GasEstimateGasPremium::NAME, Access::Read);
    access.insert(gas::GasEstimateFeeCap::NAME, Access::Read);
    access.insert(gas::GasEstimateMessageGas::NAME, Access::Read);

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
    access.insert(eth::EthAccounts::NAME, Access::Read);
    access.insert(eth::EthBlockNumber::NAME, Access::Read);
    access.insert(eth::EthChainId::NAME, Access::Read);
    access.insert(eth::EthGasPrice::NAME, Access::Read);
    access.insert(eth::EthGetBalance::NAME, Access::Read);
    access.insert(eth::EthSyncing::NAME, Access::Read);
    access.insert(eth::EthGetBlockByNumber::NAME, Access::Read);
    access.insert(eth::Web3ClientVersion::NAME, Access::Read);

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
            let token = token
                .to_str()
                .map_err(|_| ErrorCode::ParseError)?
                .trim_start_matches("Bearer ");

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

#[cfg(test)]
mod tests {
    use self::chain::ChainHead;
    use super::*;
    use chrono::Duration;

    #[tokio::test]
    async fn check_permissions_no_header() {
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));

        let res = check_permissions(keystore.clone(), None, ChainHead::NAME).await;
        assert!(res.is_ok());

        let res = check_permissions(keystore.clone(), None, "Cthulhu.InvokeElderGods").await;
        assert_eq!(res.unwrap_err(), ErrorCode::MethodNotFound);

        let res = check_permissions(keystore.clone(), None, wallet::WalletNew::NAME).await;
        assert_eq!(res.unwrap_err(), ErrorCode::InvalidRequest);
    }

    #[tokio::test]
    async fn check_permissions_invalid_header() {
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));

        let auth_header = HeaderValue::from_static("Bearer Azathoth");
        let res = check_permissions(keystore.clone(), Some(auth_header), ChainHead::NAME).await;
        assert_eq!(res.unwrap_err(), ErrorCode::InvalidRequest);

        let auth_header = HeaderValue::from_static("Cthulhu");
        let res = check_permissions(keystore.clone(), Some(auth_header), ChainHead::NAME).await;
        assert_eq!(res.unwrap_err(), ErrorCode::InvalidRequest);
    }

    #[tokio::test]
    async fn check_permissions_valid_header() {
        use crate::auth::*;
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));

        // generate a key and store it in the keystore
        let key_info = generate_priv_key();
        keystore
            .write()
            .await
            .put(JWT_IDENTIFIER, key_info.clone())
            .unwrap();
        let token_exp = Duration::hours(1);
        let token = create_token(
            ADMIN.iter().map(ToString::to_string).collect(),
            key_info.private_key(),
            token_exp,
        )
        .unwrap();

        // Should work with the `Bearer` prefix
        let auth_header = HeaderValue::from_str(&format!("Bearer {token}")).unwrap();
        let res =
            check_permissions(keystore.clone(), Some(auth_header.clone()), ChainHead::NAME).await;
        assert!(res.is_ok());

        let res = check_permissions(
            keystore.clone(),
            Some(auth_header.clone()),
            wallet::WalletNew::NAME,
        )
        .await;
        assert!(res.is_ok());

        // Should work without the `Bearer` prefix
        let auth_header = HeaderValue::from_str(&token).unwrap();
        let res =
            check_permissions(keystore.clone(), Some(auth_header), wallet::WalletNew::NAME).await;
        assert!(res.is_ok());
    }
}
