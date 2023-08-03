// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod auth_api;
mod beacon_api;
mod chain_api;
mod common_api;
mod db_api;
mod gas_api;
mod mpool_api;
mod net_api;
mod node_api;
mod progress_api;
mod rpc_http_handler;
mod rpc_util;
mod rpc_ws_handler;
mod state_api;
mod sync_api;
mod wallet_api;

use std::{net::TcpListener, sync::Arc};

use crate::rpc_api::{
    auth_api::*, beacon_api::*, chain_api::*, common_api::*, data_types::RPCState, db_api::*,
    gas_api::*, mpool_api::*, net_api::*, node_api::NODE_STATUS, progress_api::GET_PROGRESS,
    state_api::*, sync_api::*, wallet_api::*,
};
use axum::routing::{get, post};
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JSONRPCError, Params, Server};
use tokio::sync::mpsc::Sender;
use tracing::info;

use crate::rpc::{
    beacon_api::beacon_get_entry,
    common_api::{shutdown, start_time, version},
    rpc_http_handler::rpc_http_handler,
    rpc_ws_handler::rpc_ws_handler,
    state_api::*,
};

pub type RpcResult<T> = Result<T, JSONRPCError>;

pub async fn start_rpc<DB>(
    state: Arc<RPCState<DB>>,
    rpc_endpoint: TcpListener,
    forest_version: &'static str,
    shutdown_send: Sender<()>,
) -> Result<(), JSONRPCError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
{
    use auth_api::*;
    use chain_api::*;
    use gas_api::*;
    use mpool_api::*;
    use sync_api::*;
    use wallet_api::*;

    let block_delay = state.state_manager.chain_config().block_delay_secs;
    let rpc_server = Arc::new(
        Server::new()
            .with_data(Data(state))
            // Auth API
            .with_method(AUTH_NEW, auth_new::<DB>)
            .with_method(AUTH_VERIFY, auth_verify::<DB>)
            // Beacon API
            .with_method(BEACON_GET_ENTRY, beacon_get_entry::<DB>)
            // Chain API
            .with_method(CHAIN_GET_MESSAGE, chain_api::chain_get_message::<DB>)
            .with_method(CHAIN_EXPORT, chain_api::chain_export::<DB>)
            .with_method(CHAIN_READ_OBJ, chain_read_obj::<DB>)
            .with_method(CHAIN_HAS_OBJ, chain_has_obj::<DB>)
            .with_method(CHAIN_GET_BLOCK_MESSAGES, chain_get_block_messages::<DB>)
            .with_method(CHAIN_GET_TIPSET_BY_HEIGHT, chain_get_tipset_by_height::<DB>)
            .with_method(CHAIN_GET_GENESIS, chain_get_genesis::<DB>)
            .with_method(CHAIN_GET_TIPSET, chain_get_tipset::<DB>)
            .with_method(CHAIN_HEAD, chain_head::<DB>)
            .with_method(CHAIN_GET_BLOCK, chain_api::chain_get_block::<DB>)
            .with_method(CHAIN_GET_NAME, chain_api::chain_get_name::<DB>)
            .with_method(CHAIN_SET_HEAD, chain_api::chain_set_head::<DB>)
            // Message Pool API
            .with_method(MPOOL_PENDING, mpool_pending::<DB>)
            .with_method(MPOOL_PUSH, mpool_push::<DB>)
            .with_method(MPOOL_PUSH_MESSAGE, mpool_push_message::<DB>)
            // Sync API
            .with_method(SYNC_CHECK_BAD, sync_check_bad::<DB>)
            .with_method(SYNC_MARK_BAD, sync_mark_bad::<DB>)
            .with_method(SYNC_STATE, sync_state::<DB>)
            // Wallet API
            .with_method(WALLET_BALANCE, wallet_balance::<DB>)
            .with_method(WALLET_DEFAULT_ADDRESS, wallet_default_address::<DB>)
            .with_method(WALLET_EXPORT, wallet_export::<DB>)
            .with_method(WALLET_HAS, wallet_has::<DB>)
            .with_method(WALLET_IMPORT, wallet_import::<DB>)
            .with_method(WALLET_LIST, wallet_list::<DB>)
            .with_method(WALLET_NEW, wallet_new::<DB>)
            .with_method(WALLET_SET_DEFAULT, wallet_set_default::<DB>)
            .with_method(WALLET_SIGN, wallet_sign::<DB>)
            .with_method(WALLET_VERIFY, wallet_verify::<DB>)
            // State API
            .with_method(STATE_CALL, state_call::<DB>)
            .with_method(STATE_REPLAY, state_replay::<DB>)
            .with_method(STATE_NETWORK_NAME, state_network_name::<DB>)
            .with_method(STATE_NETWORK_VERSION, state_get_network_version::<DB>)
            .with_method(STATE_REPLAY, state_replay::<DB>)
            .with_method(STATE_MARKET_BALANCE, state_market_balance::<DB>)
            .with_method(STATE_MARKET_DEALS, state_market_deals::<DB>)
            .with_method(STATE_GET_RECEIPT, state_get_receipt::<DB>)
            .with_method(STATE_WAIT_MSG, state_wait_msg::<DB>)
            .with_method(STATE_FETCH_ROOT, state_fetch_root::<DB>)
            // Gas API
            .with_method(GAS_ESTIMATE_FEE_CAP, gas_estimate_fee_cap::<DB>)
            .with_method(GAS_ESTIMATE_GAS_LIMIT, gas_estimate_gas_limit::<DB>)
            .with_method(GAS_ESTIMATE_GAS_PREMIUM, gas_estimate_gas_premium::<DB>)
            .with_method(GAS_ESTIMATE_MESSAGE_GAS, gas_estimate_message_gas::<DB>)
            // Common API
            .with_method(VERSION, move || version(block_delay, forest_version))
            .with_method(SHUTDOWN, move || shutdown(shutdown_send.clone()))
            .with_method(START_TIME, start_time::<DB>)
            // Net API
            .with_method(NET_ADDRS_LISTEN, net_api::net_addrs_listen::<DB>)
            .with_method(NET_PEERS, net_api::net_peers::<DB>)
            .with_method(NET_INFO, net_api::net_info::<DB>)
            .with_method(NET_CONNECT, net_api::net_connect::<DB>)
            .with_method(NET_DISCONNECT, net_api::net_disconnect::<DB>)
            // DB API
            .with_method(DB_GC, db_api::db_gc::<DB>)
            // Progress API
            .with_method(GET_PROGRESS, progress_api::get_progress)
            // Node API
            .with_method(NODE_STATUS, node_api::node_status::<DB>)
            .finish_unwrapped(),
    );

    let app = axum::Router::new()
        .route("/rpc/v0", get(rpc_ws_handler))
        .route("/rpc/v0", post(rpc_http_handler))
        .with_state(rpc_server);

    info!("Ready for RPC connections");
    let server = axum::Server::from_tcp(rpc_endpoint)?.serve(app.into_make_service());
    server.await?;

    info!("Stopped accepting RPC connections");

    Ok(())
}
