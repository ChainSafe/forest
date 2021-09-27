// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod auth_api;
mod beacon_api;
mod chain_api;
mod common_api;
mod gas_api;
mod mpool_api;
mod net_api;
mod rpc_http_handler;
mod rpc_util;
mod rpc_ws_handler;
mod state_api;
mod sync_api;
mod wallet_api;

use async_std::sync::Arc;
use jsonrpc_v2::{Data, Error as JSONRPCError, Server};
use log::info;
use tide_websockets::WebSocket;

use beacon::Beacon;
use blockstore::BlockStore;
use fil_types::verifier::ProofVerifier;
use rpc_api::data_types::RPCState;

use crate::rpc_http_handler::rpc_http_handler;
use crate::rpc_ws_handler::rpc_ws_handler;
use crate::{beacon_api::beacon_get_entry, common_api::version, state_api::*};

use rpc_api::{
    auth_api::*, beacon_api::*, chain_api::*, common_api::*, gas_api::*, mpool_api::*, net_api::*,
    state_api::*, sync_api::*, wallet_api::*,
};

pub async fn start_rpc<DB, B, V>(
    state: Arc<RPCState<DB, B>>,
    rpc_endpoint: &str,
) -> Result<(), JSONRPCError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
{
    use auth_api::*;
    use chain_api::*;
    use gas_api::*;
    use mpool_api::*;
    use sync_api::*;
    use wallet_api::*;

    let rpc_server = Arc::new(
        Server::new()
            .with_data(Data(state))
            // Auth API
            .with_method(AUTH_NEW, auth_new::<DB, B>)
            .with_method(AUTH_VERIFY, auth_verify::<DB, B>)
            // Beacon API
            .with_method(BEACON_GET_ENTRY, beacon_get_entry::<DB, B>)
            // Chain API
            .with_method(CHAIN_GET_MESSAGE, chain_api::chain_get_message::<DB, B>)
            .with_method(CHAIN_READ_OBJ, chain_read_obj::<DB, B>)
            .with_method(CHAIN_HAS_OBJ, chain_has_obj::<DB, B>)
            .with_method(CHAIN_GET_BLOCK_MESSAGES, chain_get_block_messages::<DB, B>)
            .with_method(
                CHAIN_GET_TIPSET_BY_HEIGHT,
                chain_get_tipset_by_height::<DB, B>,
            )
            .with_method(CHAIN_GET_GENESIS, chain_get_genesis::<DB, B>)
            .with_method(CHAIN_TIPSET_WEIGHT, chain_tipset_weight::<DB, B>)
            .with_method(CHAIN_GET_TIPSET, chain_get_tipset::<DB, B>)
            .with_method(CHAIN_HEAD, chain_head::<DB, B>)
            .with_method(CHAIN_HEAD_SUBSCRIPTION, chain_head_subscription::<DB, B>)
            // * Filecoin.ChainNotify is handled specifically in middleware for streaming
            .with_method(CHAIN_NOTIFY, chain_notify::<DB, B>)
            .with_method(
                CHAIN_GET_RANDOMNESS_FROM_TICKETS,
                chain_get_randomness_from_tickets::<DB, B>,
            )
            .with_method(
                CHAIN_GET_RANDOMNESS_FROM_BEACON,
                chain_get_randomness_from_beacon::<DB, B>,
            )
            .with_method(CHAIN_GET_BLOCK, chain_api::chain_get_block::<DB, B>)
            // Message Pool API
            .with_method(MPOOL_ESTIMATE_GAS_PRICE, estimate_gas_premium::<DB, B>)
            .with_method(MPOOL_GET_NONCE, mpool_get_sequence::<DB, B>)
            .with_method(MPOOL_PENDING, mpool_pending::<DB, B>)
            .with_method(MPOOL_PUSH, mpool_push::<DB, B>)
            .with_method(MPOOL_PUSH_MESSAGE, mpool_push_message::<DB, B, V>)
            .with_method(MPOOL_SELECT, mpool_select::<DB, B>)
            // Sync API
            .with_method(SYNC_CHECK_BAD, sync_check_bad::<DB, B>)
            .with_method(SYNC_MARK_BAD, sync_mark_bad::<DB, B>)
            .with_method(SYNC_STATE, sync_state::<DB, B>)
            .with_method(SYNC_SUBMIT_BLOCK, sync_submit_block::<DB, B>)
            // Wallet API
            .with_method(WALLET_BALANCE, wallet_balance::<DB, B>)
            .with_method(WALLET_DEFAULT_ADDRESS, wallet_default_address::<DB, B>)
            .with_method(WALLET_EXPORT, wallet_export::<DB, B>)
            .with_method(WALLET_HAS, wallet_has::<DB, B>)
            .with_method(WALLET_IMPORT, wallet_import::<DB, B>)
            .with_method(WALLET_LIST, wallet_list::<DB, B>)
            .with_method(WALLET_NEW, wallet_new::<DB, B>)
            .with_method(WALLET_SET_DEFAULT, wallet_set_default::<DB, B>)
            .with_method(WALLET_SIGN, wallet_sign::<DB, B>)
            .with_method(WALLET_SIGN_MESSAGE, wallet_sign_message::<DB, B>)
            .with_method(WALLET_VERIFY, wallet_verify::<DB, B>)
            // State API
            .with_method(STATE_MINER_SECTORS, state_miner_sectors::<DB, B>)
            .with_method(STATE_CALL, state_call::<DB, B>)
            .with_method(STATE_MINER_DEADLINES, state_miner_deadlines::<DB, B>)
            .with_method(
                STATE_SECTOR_PRECOMMIT_INFO,
                state_sector_precommit_info::<DB, B>,
            )
            .with_method(STATE_MINER_INFO, state_miner_info::<DB, B>)
            .with_method(STATE_SECTOR_GET_INFO, state_sector_info::<DB, B>)
            .with_method(
                STATE_MINER_PROVING_DEADLINE,
                state_miner_proving_deadline::<DB, B>,
            )
            .with_method(STATE_MINER_FAULTS, state_miner_faults::<DB, B>)
            .with_method(STATE_ALL_MINER_FAULTS, state_all_miner_faults::<DB, B>)
            .with_method(STATE_MINER_RECOVERIES, state_miner_recoveries::<DB, B>)
            .with_method(STATE_MINER_PARTITIONS, state_miner_partitions::<DB, B>)
            .with_method(STATE_REPLAY, state_replay::<DB, B>)
            .with_method(STATE_NETWORK_NAME, state_network_name::<DB, B>)
            .with_method(STATE_NETWORK_VERSION, state_get_network_version::<DB, B>)
            .with_method(STATE_REPLAY, state_replay::<DB, B>)
            .with_method(STATE_GET_ACTOR, state_get_actor::<DB, B, V>)
            .with_method(STATE_ACCOUNT_KEY, state_account_key::<DB, B, V>)
            .with_method(STATE_LOOKUP_ID, state_lookup_id::<DB, B, V>)
            .with_method(STATE_MARKET_BALANCE, state_market_balance::<DB, B>)
            .with_method(STATE_MARKET_DEALS, state_market_deals::<DB, B>)
            .with_method(STATE_GET_RECEIPT, state_get_receipt::<DB, B>)
            .with_method(STATE_WAIT_MSG, state_wait_msg::<DB, B>)
            .with_method(MINER_CREATE_BLOCK, miner_create_block::<DB, B, V>)
            .with_method(
                STATE_MINER_SECTOR_ALLOCATED,
                state_miner_sector_allocated::<DB, B>,
            )
            .with_method(
                STATE_MINER_PRE_COMMIT_DEPOSIT_FOR_POWER,
                state_miner_pre_commit_deposit_for_power::<DB, B, V>,
            )
            .with_method(
                STATE_MINER_INITIAL_PLEDGE_COLLATERAL,
                state_miner_initial_pledge_collateral::<DB, B, V>,
            )
            .with_method(MINER_GET_BASE_INFO, miner_get_base_info::<DB, B, V>)
            // Gas API
            .with_method(GAS_ESTIMATE_FEE_CAP, gas_estimate_fee_cap::<DB, B>)
            .with_method(GAS_ESTIMATE_GAS_LIMIT, gas_estimate_gas_limit::<DB, B, V>)
            .with_method(GAS_ESTIMATE_GAS_PREMIUM, gas_estimate_gas_premium::<DB, B>)
            .with_method(
                GAS_ESTIMATE_MESSAGE_GAS,
                gas_estimate_message_gas::<DB, B, V>,
            )
            // Common API
            .with_method(VERSION, version)
            // Net API
            .with_method(NET_ADDRS_LISTEN, net_api::net_addrs_listen::<DB, B>)
            .with_method(NET_PEERS, net_api::net_peers::<DB, B>)
            .with_method(NET_CONNECT, net_api::net_connect::<DB, B>)
            .with_method(NET_DISCONNECT, net_api::net_disconnect::<DB, B>)
            .finish_unwrapped(),
    );

    let mut app = tide::with_state(Arc::clone(&rpc_server));

    app.at("/rpc/v0")
        .get(WebSocket::new(rpc_ws_handler::<DB, B>))
        .post(rpc_http_handler::<DB, B>);

    info!("Ready for RPC connections");

    app.listen(rpc_endpoint).await?;

    info!("Stopped accepting RPC connections");

    Ok(())
}
