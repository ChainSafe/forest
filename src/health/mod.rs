// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::sync::Arc;

use axum::{
    Router,
    response::{IntoResponse, Response},
    routing::get,
};
use parking_lot::RwLock;

use crate::chain_sync::ForestSyncStatusReport;
use crate::{Config, db::SettingsStore, libp2p::PeerManager, networks::ChainConfig};

mod endpoints;

/// Default listening port for the healthcheck server.
pub const DEFAULT_HEALTHCHECK_PORT: u16 = 2346;

/// State shared between the healthcheck server and the main application.
pub(crate) struct ForestState {
    pub config: Config,
    pub chain_config: Arc<ChainConfig>,
    pub genesis_timestamp: u64,
    pub sync_status: Arc<RwLock<ForestSyncStatusReport>>,
    pub peer_manager: Arc<PeerManager>,
    pub settings_store: Arc<dyn SettingsStore + Sync + Send>,
}

/// Initializes the healthcheck server. The server listens on the address specified in the
/// configuration (passed via state) and responds to the following endpoints:
/// - `[endpoints::healthz]`
/// - `[endpoints::readyz]`
/// - `[endpoints::livez]`
///
/// All endpoints accept an optional `verbose` query parameter. If present, the response will include detailed information about the checks performed.
pub(crate) async fn init_healthcheck_server(
    forest_state: ForestState,
    tcp_listener: tokio::net::TcpListener,
) -> anyhow::Result<()> {
    let healthcheck_service = Router::new()
        .route("/healthz", get(endpoints::healthz))
        .route("/readyz", get(endpoints::readyz))
        .route("/livez", get(endpoints::livez))
        .with_state(forest_state.into());

    axum::serve(tcp_listener, healthcheck_service).await?;
    Ok(())
}

/// Simple error wrapper for the healthcheck server
struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (http::StatusCode::SERVICE_UNAVAILABLE, self.0.to_string()).into_response()
    }
}

#[cfg(test)]
mod test {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use crate::Client;
    use crate::cli_shared::cli::ChainIndexerConfig;
    use crate::db::SettingsExt;

    use super::*;
    use crate::chain_sync::NodeSyncStatus;
    use reqwest::StatusCode;

    #[tokio::test]
    async fn test_check_readyz() {
        let healthcheck_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let rpc_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

        let sync_status = Arc::new(RwLock::new(ForestSyncStatusReport::init()));
        let db = Arc::new(crate::db::MemoryDB::default());

        let forest_state = ForestState {
            config: Config {
                chain_indexer: ChainIndexerConfig {
                    enable_indexer: true,
                    gc_retention_epochs: None,
                },
                client: Client {
                    healthcheck_address,
                    rpc_address: rpc_listener.local_addr().unwrap(),
                    ..Default::default()
                },
                ..Default::default()
            },
            chain_config: Arc::new(ChainConfig::default()),
            genesis_timestamp: 0,
            sync_status: sync_status.clone(),
            peer_manager: Arc::new(PeerManager::default()),
            settings_store: db.clone(),
        };

        let listener =
            tokio::net::TcpListener::bind(forest_state.config.client.healthcheck_address)
                .await
                .unwrap();
        let healthcheck_port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            init_healthcheck_server(forest_state, listener)
                .await
                .unwrap();
        });

        let call_healthcheck = |verbose| {
            reqwest::get(format!(
                "http://localhost:{}/readyz{}",
                healthcheck_port,
                if verbose { "?verbose" } else { "" }
            ))
        };

        // instrument the state so that the ready requirements are met
        sync_status.write().set_status(NodeSyncStatus::Synced);
        sync_status.write().current_head_epoch = i64::MAX;
        db.set_eth_mapping_up_to_date().unwrap();

        assert_eq!(
            call_healthcheck(false).await.unwrap().status(),
            StatusCode::OK
        );
        let response = call_healthcheck(true).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let text = response.text().await.unwrap();
        assert!(text.contains("[+] sync complete"));
        assert!(text.contains("[+] epoch up to date"));
        assert!(text.contains("[+] rpc server running"));
        assert!(text.contains("[+] eth mappings up to date"));

        // instrument the state so that the ready requirements are not met
        drop(rpc_listener);
        sync_status.write().set_status(NodeSyncStatus::Error);
        sync_status.write().current_head_epoch = 0;

        assert_eq!(
            call_healthcheck(false).await.unwrap().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        let response = call_healthcheck(true).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let text = response.text().await.unwrap();
        assert!(text.contains("[!] sync incomplete"));
        assert!(text.contains("[!] epoch outdated"));
        assert!(text.contains("[!] rpc server not running"));
        assert!(text.contains("[+] eth mappings up to date"));
    }

    #[tokio::test]
    async fn test_check_livez() {
        let healthcheck_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let rpc_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

        let sync_status = Arc::new(RwLock::new(ForestSyncStatusReport::default()));
        let peer_manager = Arc::new(PeerManager::default());
        let db = Arc::new(crate::db::MemoryDB::default());
        let forest_state = ForestState {
            config: Config {
                client: Client {
                    healthcheck_address,
                    rpc_address: rpc_listener.local_addr().unwrap(),
                    ..Default::default()
                },
                ..Default::default()
            },
            chain_config: Arc::new(ChainConfig::default()),
            genesis_timestamp: 0,
            sync_status: sync_status.clone(),
            peer_manager: peer_manager.clone(),
            settings_store: db,
        };

        let listener =
            tokio::net::TcpListener::bind(forest_state.config.client.healthcheck_address)
                .await
                .unwrap();
        let healthcheck_port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            init_healthcheck_server(forest_state, listener)
                .await
                .unwrap();
        });

        let call_healthcheck = |verbose| {
            reqwest::get(format!(
                "http://localhost:{}/livez{}",
                healthcheck_port,
                if verbose { "?verbose" } else { "" }
            ))
        };

        // instrument the state so that the live requirements are met
        sync_status.write().set_status(NodeSyncStatus::Syncing);
        let peer = libp2p::PeerId::random();
        peer_manager.touch_peer(&peer);

        assert_eq!(
            call_healthcheck(false).await.unwrap().status(),
            StatusCode::OK
        );

        let response = call_healthcheck(true).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let text = response.text().await.unwrap();
        assert!(text.contains("[+] sync ok"));
        assert!(text.contains("[+] peers connected"));

        // instrument the state so that the live requirements are not met
        sync_status.write().set_status(NodeSyncStatus::Error);
        peer_manager.remove_peer(&peer);

        assert_eq!(
            call_healthcheck(false).await.unwrap().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );

        let response = call_healthcheck(true).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let text = response.text().await.unwrap();
        assert!(text.contains("[!] sync error"));
        assert!(text.contains("[!] no peers connected"));
    }

    #[tokio::test]
    async fn test_check_healthz() {
        let healthcheck_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let rpc_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer_manager = Arc::new(PeerManager::default());
        let db = Arc::new(crate::db::MemoryDB::default());

        let sync_status = Arc::new(RwLock::new(ForestSyncStatusReport::default()));
        let forest_state = ForestState {
            config: Config {
                client: Client {
                    healthcheck_address,
                    rpc_address: rpc_listener.local_addr().unwrap(),
                    ..Default::default()
                },
                ..Default::default()
            },
            chain_config: Arc::new(ChainConfig::default()),
            genesis_timestamp: 0,
            sync_status: sync_status.clone(),
            peer_manager: peer_manager.clone(),
            settings_store: db,
        };

        let listener =
            tokio::net::TcpListener::bind(forest_state.config.client.healthcheck_address)
                .await
                .unwrap();
        let healthcheck_port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            init_healthcheck_server(forest_state, listener)
                .await
                .unwrap();
        });

        let call_healthcheck = |verbose| {
            reqwest::get(format!(
                "http://localhost:{}/healthz{}",
                healthcheck_port,
                if verbose { "?verbose" } else { "" }
            ))
        };

        // instrument the state so that the health requirements are met
        sync_status.write().current_head_epoch = i64::MAX;
        sync_status.write().set_status(NodeSyncStatus::Syncing);
        let peer = libp2p::PeerId::random();
        peer_manager.touch_peer(&peer);

        assert_eq!(
            call_healthcheck(false).await.unwrap().status(),
            StatusCode::OK
        );
        let response = call_healthcheck(true).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let text = response.text().await.unwrap();
        assert!(text.contains("[+] sync ok"));
        assert!(text.contains("[+] epoch up to date"));
        assert!(text.contains("[+] rpc server running"));
        assert!(text.contains("[+] peers connected"));

        // instrument the state so that the health requirements are not met
        drop(rpc_listener);
        sync_status.write().set_status(NodeSyncStatus::Error);
        sync_status.write().current_head_epoch = 0;
        peer_manager.remove_peer(&peer);

        assert_eq!(
            call_healthcheck(false).await.unwrap().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        let response = call_healthcheck(true).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let text = response.text().await.unwrap();
        assert!(text.contains("[!] sync error"));
        assert!(text.contains("[!] epoch outdated"));
        assert!(text.contains("[!] rpc server not running"));
        assert!(text.contains("[!] no peers connected"));
    }

    #[tokio::test]
    async fn test_check_unknown_healthcheck_endpoint() {
        let healthcheck_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let forest_state = ForestState {
            config: Config {
                client: Client {
                    healthcheck_address,
                    ..Default::default()
                },
                ..Default::default()
            },
            chain_config: Arc::default(),
            genesis_timestamp: 0,
            sync_status: Arc::new(RwLock::new(ForestSyncStatusReport::default())),
            peer_manager: Arc::default(),
            settings_store: Arc::new(crate::db::MemoryDB::default()),
        };
        let listener =
            tokio::net::TcpListener::bind(forest_state.config.client.healthcheck_address)
                .await
                .unwrap();
        let healthcheck_port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            init_healthcheck_server(forest_state, listener)
                .await
                .unwrap();
        });

        let response = reqwest::get(format!(
            "http://localhost:{}/phngluimglwnafhcthulhurlyehwgahnaglfhtagn",
            healthcheck_port
        ))
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
