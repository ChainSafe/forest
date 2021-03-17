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
use wallet::KeyStore;

pub use crate::data_types::RpcState;
use crate::rpc_http_handler::rpc_http_handler;
use crate::rpc_ws_handler::rpc_ws_handler;
use crate::{beacon_api::beacon_get_entry, common_api::version, state_api::*};

pub async fn start_rpc<DB, KS, B, V>(
    state: Arc<RpcState<DB, KS, B>>,
    rpc_endpoint: &str,
) -> Result<(), JSONRPCError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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
            .with_method("Filecoin.AuthNew", auth_new::<DB, KS, B>)
            .with_method("Filecoin.AuthVerify", auth_verify::<DB, KS, B>)
            // Chain API
            .with_method(
                "Filecoin.ChainGetMessage",
                chain_api::chain_get_message::<DB, KS, B>,
            )
            .with_method("Filecoin.ChainReadObj", chain_read_obj::<DB, KS, B>)
            .with_method("Filecoin.ChainHasObj", chain_has_obj::<DB, KS, B>)
            .with_method(
                "Filecoin.ChainGetBlockMessages",
                chain_block_messages::<DB, KS, B>,
            )
            .with_method(
                "Filecoin.ChainGetTipsetByHeight",
                chain_get_tipset_by_height::<DB, KS, B>,
            )
            .with_method("Filecoin.ChainGetGenesis", chain_get_genesis::<DB, KS, B>)
            .with_method(
                "Filecoin.ChainTipSetWeight",
                chain_tipset_weight::<DB, KS, B>,
            )
            .with_method("Filecoin.ChainGetTipSet", chain_get_tipset::<DB, KS, B>)
            .with_method(
                "Filecoin.ChainHeadSubscription",
                chain_head_sub::<DB, KS, B>,
            )
            .with_method("Filecoin.ChainNotify", chain_notify::<DB, KS, B>)
            .with_method(
                "Filecoin.ChainGetRandomnessFromTickets",
                chain_get_randomness_from_tickets::<DB, KS, B>,
            )
            .with_method(
                "Filecoin.ChainGetRandomnessFromBeacon",
                chain_get_randomness_from_beacon::<DB, KS, B>,
            )
            .with_method(
                "Filecoin.ChainGetBlock",
                chain_api::chain_get_block::<DB, KS, B>,
            )
            // * Filecoin.ChainNotify is handled specifically in middleware for streaming
            .with_method("Filecoin.ChainHead", chain_head::<DB, KS, B>)
            // Message Pool API
            .with_method(
                "Filecoin.MpoolEstimateGasPrice",
                estimate_gas_premium::<DB, KS, B>,
            )
            .with_method("Filecoin.MpoolGetNonce", mpool_get_sequence::<DB, KS, B>)
            .with_method("Filecoin.MpoolPending", mpool_pending::<DB, KS, B>)
            .with_method("Filecoin.MpoolPush", mpool_push::<DB, KS, B>)
            .with_method(
                "Filecoin.MpoolPushMessage",
                mpool_push_message::<DB, KS, B, V>,
            )
            .with_method("Filecoin.MpoolSelect", mpool_select::<DB, KS, B>)
            // Sync API
            .with_method("Filecoin.SyncCheckBad", sync_check_bad::<DB, KS, B>)
            .with_method("Filecoin.SyncMarkBad", sync_mark_bad::<DB, KS, B>)
            .with_method("Filecoin.SyncState", sync_state::<DB, KS, B>)
            .with_method("Filecoin.SyncSubmitBlock", sync_submit_block::<DB, KS, B>)
            // Wallet API
            .with_method("Filecoin.WalletBalance", wallet_balance::<DB, KS, B>)
            .with_method(
                "Filecoin.WalletDefaultAddress",
                wallet_default_address::<DB, KS, B>,
            )
            .with_method("Filecoin.WalletExport", wallet_export::<DB, KS, B>)
            .with_method("Filecoin.WalletHas", wallet_has::<DB, KS, B>)
            .with_method("Filecoin.WalletImport", wallet_import::<DB, KS, B>)
            .with_method("Filecoin.WalletList", wallet_list::<DB, KS, B>)
            .with_method("Filecoin.WalletNew", wallet_new::<DB, KS, B>)
            .with_method("Filecoin.WalletSetDefault", wallet_set_default::<DB, KS, B>)
            .with_method("Filecoin.WalletSign", wallet_sign::<DB, KS, B>)
            .with_method(
                "Filecoin.WalletSignMessage",
                wallet_sign_message::<DB, KS, B>,
            )
            .with_method("Filecoin.WalletVerify", wallet_verify::<DB, KS, B>)
            // State API
            .with_method(
                "Filecoin.StateMinerSectors",
                state_miner_sectors::<DB, KS, B>,
            )
            .with_method("Filecoin.StateCall", state_call::<DB, KS, B>)
            .with_method(
                "Filecoin.StateMinerDeadlines",
                state_miner_deadlines::<DB, KS, B>,
            )
            .with_method(
                "Filecoin.StateSectorPrecommitInfo",
                state_sector_precommit_info::<DB, KS, B>,
            )
            .with_method(
                "Filecoin.StateSectorGetInfo",
                state_sector_info::<DB, KS, B>,
            )
            .with_method(
                "Filecoin.StateMinerProvingDeadline",
                state_miner_proving_deadline::<DB, KS, B>,
            )
            .with_method("Filecoin.StateMinerInfo", state_miner_info::<DB, KS, B>)
            .with_method("Filecoin.StateMinerFaults", state_miner_faults::<DB, KS, B>)
            .with_method(
                "Filecoin.StateAllMinerFaults",
                state_all_miner_faults::<DB, KS, B>,
            )
            .with_method(
                "Filecoin.StateMinerRecoveries",
                state_miner_recoveries::<DB, KS, B>,
            )
            .with_method(
                "Filecoin.StateMinerPartitions",
                state_miner_partitions::<DB, KS, B>,
            )
            .with_method(
                "Filecoin.StateMinerPreCommitDepositForPower",
                state_miner_pre_commit_deposit_for_power::<DB, KS, B, V>,
            )
            .with_method(
                "Filecoin.StateMinerInitialPledgeCollateral",
                state_miner_initial_pledge_collateral::<DB, KS, B, V>,
            )
            .with_method("Filecoin.StateReplay", state_replay::<DB, KS, B>)
            .with_method("Filecoin.StateGetActor", state_get_actor::<DB, KS, B, V>)
            .with_method(
                "Filecoin.StateAccountKey",
                state_account_key::<DB, KS, B, V>,
            )
            .with_method("Filecoin.StateLookupId", state_lookup_id::<DB, KS, B, V>)
            .with_method(
                "Filecoin.StateMarketBalance",
                state_market_balance::<DB, KS, B>,
            )
            .with_method("Filecoin.StateMarketDeals", state_market_deals::<DB, KS, B>)
            .with_method("Filecoin.StateGetReceipt", state_get_receipt::<DB, KS, B>)
            .with_method("Filecoin.StateWaitMsg", state_wait_msg::<DB, KS, B>)
            .with_method(
                "Filecoin.StateMinerSectorAllocated",
                state_miner_sector_allocated::<DB, KS, B>,
            )
            .with_method("Filecoin.StateNetworkName", state_network_name::<DB, KS, B>)
            .with_method(
                "Filecoin.MinerGetBaseInfo",
                state_miner_get_base_info::<DB, KS, B, V>,
            )
            .with_method(
                "Filecoin.MinerCreateBlock",
                miner_create_block::<DB, KS, B, V>,
            )
            .with_method(
                "Filecoin.StateNetworkVersion",
                state_get_network_version::<DB, KS, B>,
            )
            // Gas API
            .with_method(
                "Filecoin.GasEstimateGasLimit",
                gas_estimate_gas_limit::<DB, KS, B, V>,
            )
            .with_method(
                "Filecoin.GasEstimateGasPremium",
                gas_estimate_gas_premium::<DB, KS, B>,
            )
            .with_method(
                "Filecoin.GasEstimateFeeCap",
                gas_estimate_fee_cap::<DB, KS, B>,
            )
            .with_method(
                "Filecoin.GasEstimateMessageGas",
                gas_estimate_message_gas::<DB, KS, B, V>,
            )
            // Common
            .with_method("Filecoin.Version", version)
            //beacon
            .with_method("Filecoin.BeaconGetEntry", beacon_get_entry::<DB, KS, B>)
            // Net
            .with_method(
                "Filecoin.NetAddrsListen",
                net_api::net_addrs_listen::<DB, KS, B>,
            )
            .finish_unwrapped(),
    );

    let mut app = tide::with_state(Arc::clone(&rpc_server));

    app.at("/rpc/v0")
        .get(WebSocket::new(rpc_ws_handler::<DB, KS, B>))
        .post(rpc_http_handler::<DB, KS, B>);

    info!("Ready for RPC connections");

    app.listen(rpc_endpoint).await?;

    info!("Stopped accepting RPC connections");

    Ok(())
}
