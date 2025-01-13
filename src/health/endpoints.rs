// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::sync::Arc;

use ahash::HashMap;
use axum::extract::{self, Query};

use crate::db::SettingsExt;
use crate::{chain_sync::SyncStage, networks::calculate_expected_epoch};

use super::{AppError, ForestState};

/// Query parameter for verbose responses
const VERBOSE_PARAM: &str = "verbose";

/// Liveness probes determine whether or not an application running in a container is in a healthy state. The idea behind a liveness probe is that it fails for prolonged period of time, then the application should be restarted.
/// In our case, we require:
/// - The node is not in an error state (i.e., boot-looping)
/// - At least 1 peer is connected (without peers, the node is isolated and cannot sync)
///
/// If any of these conditions are not met, the node is **not** healthy. If this happens for a prolonged period of time, the application should be restarted.
pub(crate) async fn livez(
    extract::State(state): extract::State<Arc<ForestState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<String, AppError> {
    let mut acc = MessageAccumulator::new_with_enabled(params.contains_key(VERBOSE_PARAM));

    let mut lively = true;
    lively &= check_sync_state_not_error(&state, &mut acc);
    lively &= check_peers_connected(&state, &mut acc);

    if lively {
        Ok(acc.result_ok())
    } else {
        Err(AppError(anyhow::anyhow!(acc.result_err())))
    }
}

/// Readiness probes determine whether or not a container is ready to serve requests.
/// The goal is to determine if the application is fully prepared to accept traffic.
/// In our case, we require:
/// - The node is in sync with the network
/// - The current epoch of the node is not too far behind the network
/// - The RPC server is running
/// - The Ethereum mapping is up to date
///
/// If any of these conditions are not met, the nod is **not** ready to serve requests.
pub(crate) async fn readyz(
    extract::State(state): extract::State<Arc<ForestState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<String, AppError> {
    let mut acc = MessageAccumulator::new_with_enabled(params.contains_key(VERBOSE_PARAM));

    let mut ready = true;
    ready &= check_sync_state_complete(&state, &mut acc);
    ready &= check_epoch_up_to_date(&state, &mut acc);
    ready &= check_rpc_server_running(&state, &mut acc).await;
    ready &= check_eth_mapping_up_to_date(&state, &mut acc);

    if ready {
        Ok(acc.result_ok())
    } else {
        Err(AppError(anyhow::anyhow!(acc.result_err())))
    }
}

/// This endpoint is a combination of the `[livez]` and `[readyz]` endpoints, except that the node
/// doesn't have to be fully synced. Deprecated in the Kubernetes world, but still used in some setups.
pub(crate) async fn healthz(
    extract::State(state): extract::State<Arc<ForestState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<String, AppError> {
    let mut acc = MessageAccumulator::new_with_enabled(params.contains_key(VERBOSE_PARAM));

    let mut healthy = true;
    healthy &= check_epoch_up_to_date(&state, &mut acc);
    healthy &= check_rpc_server_running(&state, &mut acc).await;
    healthy &= check_sync_state_not_error(&state, &mut acc);
    healthy &= check_peers_connected(&state, &mut acc);

    if healthy {
        Ok(acc.result_ok())
    } else {
        Err(AppError(anyhow::anyhow!(acc.result_err())))
    }
}

fn check_sync_state_complete(state: &ForestState, acc: &mut MessageAccumulator) -> bool {
    // Forest must be in sync with the network
    if state.sync_state.read().stage() == SyncStage::Complete {
        acc.push_ok("sync complete");
        true
    } else {
        acc.push_err("sync incomplete");
        false
    }
}

fn check_sync_state_not_error(state: &ForestState, acc: &mut MessageAccumulator) -> bool {
    // Forest must be in sync with the network
    if state.sync_state.read().stage() != SyncStage::Error {
        acc.push_ok("sync ok");
        true
    } else {
        acc.push_err("sync error");
        false
    }
}

/// Checks if the current epoch of the node is not too far behind the network.
/// Making the threshold too strict can cause the node to repeatedly report as not ready, especially
/// in case of forking.
fn check_epoch_up_to_date(state: &ForestState, acc: &mut MessageAccumulator) -> bool {
    const MAX_EPOCH_DIFF: i64 = 5;

    let now_epoch = calculate_expected_epoch(
        chrono::Utc::now().timestamp() as u64,
        state.genesis_timestamp,
        state.chain_config.block_delay_secs,
    ) as i64;

    // The current epoch of the node must be not too far behind the network
    if state.sync_state.read().epoch() >= now_epoch - MAX_EPOCH_DIFF {
        acc.push_ok("epoch up to date");
        true
    } else {
        acc.push_err("epoch outdated");
        false
    }
}

async fn check_rpc_server_running(state: &ForestState, acc: &mut MessageAccumulator) -> bool {
    if !state.config.client.enable_rpc {
        acc.push_ok("rpc server disabled");
        true
    } else if tokio::net::TcpStream::connect(state.config.client.rpc_address)
        .await
        .is_ok()
    {
        acc.push_ok("rpc server running");
        true
    } else {
        acc.push_err("rpc server not running");
        false
    }
}

fn check_peers_connected(state: &ForestState, acc: &mut MessageAccumulator) -> bool {
    // At least 1 peer is connected
    if state.peer_manager.peer_count() > 0 {
        acc.push_ok("peers connected");
        true
    } else {
        acc.push_err("no peers connected");
        false
    }
}

fn check_eth_mapping_up_to_date(state: &ForestState, acc: &mut MessageAccumulator) -> bool {
    match state.settings_store.eth_mapping_up_to_date() {
        Ok(Some(true)) => {
            acc.push_ok("eth mapping up to date");
            true
        }
        Ok(None) | Ok(Some(false)) | Err(_) => {
            acc.push_err("no eth mapping");
            false
        }
    }
}

/// Sample message accumulator for healthcheck responses. It is intended to accumulate messages for
/// verbose responses.
struct MessageAccumulator {
    messages: Vec<String>,
    enabled: bool,
}

impl MessageAccumulator {
    fn new_with_enabled(enabled: bool) -> Self {
        Self {
            messages: Vec::new(),
            enabled,
        }
    }

    fn push_ok<S: AsRef<str>>(&mut self, message: S) {
        if self.enabled {
            self.messages.push(format!("[+] {}", message.as_ref()));
        }
    }

    fn push_err<S: AsRef<str>>(&mut self, message: S) {
        if self.enabled {
            self.messages.push(format!("[!] {}", message.as_ref()));
        }
    }

    fn result_ok(&self) -> String {
        if self.enabled {
            self.messages.join("\n")
        } else {
            "OK".to_string()
        }
    }

    fn result_err(&self) -> String {
        if self.enabled {
            self.messages.join("\n")
        } else {
            "ERROR".to_string()
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_message_accumulator() {
        let mut acc = MessageAccumulator::new_with_enabled(true);
        acc.push_ok("ok1");
        acc.push_err("err1");
        acc.push_ok("ok2");
        acc.push_err("err2");

        assert_eq!(acc.result_ok(), "[+] ok1\n[!] err1\n[+] ok2\n[!] err2");
        assert_eq!(acc.result_err(), "[+] ok1\n[!] err1\n[+] ok2\n[!] err2");
    }

    #[test]
    fn test_message_accumulator_disabled() {
        let mut acc = MessageAccumulator::new_with_enabled(false);
        acc.push_ok("ok1");
        acc.push_err("err1");
        acc.push_ok("ok2");
        acc.push_err("err2");

        assert_eq!(acc.result_ok(), "OK");
        assert_eq!(acc.result_err(), "ERROR");
    }
}
