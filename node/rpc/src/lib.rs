// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod auth_api;
mod beacon_api;
mod chain_api;
mod common_api;
mod data_types;
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
use log::info;

use jsonrpc_v2::{Data, Error as JSONRPCError, Server};
use tide_websockets::WebSocket;

use beacon::Beacon;
use blockstore::BlockStore;
use fil_types::verifier::ProofVerifier;

pub use crate::data_types::RpcState;
use crate::rpc_http_handler::rpc_http_handler;
use crate::rpc_ws_handler::rpc_ws_handler;
use crate::{beacon_api::beacon_get_entry, common_api::version, state_api::*};

use rpc_api::{auth_api::*, beacon_api::*, chain_api::*};

pub async fn start_rpc<DB, B, V>(
    state: Arc<RpcState<DB, B>>,
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
            .with_method(
                "Filecoin.MpoolEstimateGasPrice",
                estimate_gas_premium::<DB, B>,
            )
            .with_method("Filecoin.MpoolGetNonce", mpool_get_sequence::<DB, B>)
            .with_method("Filecoin.MpoolPending", mpool_pending::<DB, B>)
            .with_method("Filecoin.MpoolPush", mpool_push::<DB, B>)
            .with_method("Filecoin.MpoolPushMessage", mpool_push_message::<DB, B, V>)
            .with_method("Filecoin.MpoolSelect", mpool_select::<DB, B>)
            // Sync API
            .with_method("Filecoin.SyncCheckBad", sync_check_bad::<DB, B>)
            .with_method("Filecoin.SyncMarkBad", sync_mark_bad::<DB, B>)
            .with_method("Filecoin.SyncState", sync_state::<DB, B>)
            .with_method("Filecoin.SyncSubmitBlock", sync_submit_block::<DB, B>)
            // Wallet API
            .with_method("Filecoin.WalletBalance", wallet_balance::<DB, B>)
            .with_method(
                "Filecoin.WalletDefaultAddress",
                wallet_default_address::<DB, B>,
            )
            .with_method("Filecoin.WalletExport", wallet_export::<DB, B>)
            .with_method("Filecoin.WalletHas", wallet_has::<DB, B>)
            .with_method("Filecoin.WalletImport", wallet_import::<DB, B>)
            .with_method("Filecoin.WalletList", wallet_list::<DB, B>)
            .with_method("Filecoin.WalletNew", wallet_new::<DB, B>)
            .with_method("Filecoin.WalletSetDefault", wallet_set_default::<DB, B>)
            .with_method("Filecoin.WalletSign", wallet_sign::<DB, B>)
            .with_method("Filecoin.WalletSignMessage", wallet_sign_message::<DB, B>)
            .with_method("Filecoin.WalletVerify", wallet_verify::<DB, B>)
            // State API
            .with_method("Filecoin.StateMinerSectors", state_miner_sectors::<DB, B>)
            .with_method("Filecoin.StateCall", state_call::<DB, B>)
            .with_method(
                "Filecoin.StateMinerDeadlines",
                state_miner_deadlines::<DB, B>,
            )
            .with_method(
                "Filecoin.StateSectorPrecommitInfo",
                state_sector_precommit_info::<DB, B>,
            )
            .with_method("Filecoin.StateSectorGetInfo", state_sector_info::<DB, B>)
            .with_method(
                "Filecoin.StateMinerProvingDeadline",
                state_miner_proving_deadline::<DB, B>,
            )
            .with_method("Filecoin.StateMinerInfo", state_miner_info::<DB, B>)
            .with_method("Filecoin.StateMinerFaults", state_miner_faults::<DB, B>)
            .with_method(
                "Filecoin.StateAllMinerFaults",
                state_all_miner_faults::<DB, B>,
            )
            .with_method(
                "Filecoin.StateMinerRecoveries",
                state_miner_recoveries::<DB, B>,
            )
            .with_method(
                "Filecoin.StateMinerPartitions",
                state_miner_partitions::<DB, B>,
            )
            .with_method(
                "Filecoin.StateMinerPreCommitDepositForPower",
                state_miner_pre_commit_deposit_for_power::<DB, B, V>,
            )
            .with_method(
                "Filecoin.StateMinerInitialPledgeCollateral",
                state_miner_initial_pledge_collateral::<DB, B, V>,
            )
            .with_method("Filecoin.StateReplay", state_replay::<DB, B>)
            .with_method("Filecoin.StateGetActor", state_get_actor::<DB, B, V>)
            .with_method("Filecoin.StateAccountKey", state_account_key::<DB, B, V>)
            .with_method("Filecoin.StateLookupId", state_lookup_id::<DB, B, V>)
            .with_method("Filecoin.StateMarketBalance", state_market_balance::<DB, B>)
            .with_method("Filecoin.StateMarketDeals", state_market_deals::<DB, B>)
            .with_method("Filecoin.StateGetReceipt", state_get_receipt::<DB, B>)
            .with_method("Filecoin.StateWaitMsg", state_wait_msg::<DB, B>)
            .with_method(
                "Filecoin.StateMinerSectorAllocated",
                state_miner_sector_allocated::<DB, B>,
            )
            .with_method("Filecoin.StateNetworkName", state_network_name::<DB, B>)
            .with_method(
                "Filecoin.MinerGetBaseInfo",
                state_miner_get_base_info::<DB, B, V>,
            )
            .with_method("Filecoin.MinerCreateBlock", miner_create_block::<DB, B, V>)
            .with_method(
                "Filecoin.StateNetworkVersion",
                state_get_network_version::<DB, B>,
            )
            // Gas API
            .with_method(
                "Filecoin.GasEstimateGasLimit",
                gas_estimate_gas_limit::<DB, B, V>,
            )
            .with_method(
                "Filecoin.GasEstimateGasPremium",
                gas_estimate_gas_premium::<DB, B>,
            )
            .with_method("Filecoin.GasEstimateFeeCap", gas_estimate_fee_cap::<DB, B>)
            .with_method(
                "Filecoin.GasEstimateMessageGas",
                gas_estimate_message_gas::<DB, B, V>,
            )
            // Common API
            .with_method("Filecoin.Version", version)
            // Net API
            .with_method(
                "Filecoin.NetAddrsListen",
                net_api::net_addrs_listen::<DB, B>,
            )
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
