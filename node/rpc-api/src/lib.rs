// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Access levels to be checked against JWT claims
pub enum Access {
    Admin,
    Sign,
    Write,
    Read,
}

/// Access mapping between method names and access levels
/// Checked against JWT claims on every request
pub static ACCESS_MAP: Lazy<HashMap<&str, Access>> = Lazy::new(|| {
    let mut access = HashMap::new();

    access.insert(auth_api::AUTH_NEW, Access::Admin);
    access.insert(auth_api::AUTH_VERIFY, Access::Read);
    access.insert(beacon_api::BEACON_GET_ENTRY, Access::Read);
    access.insert(chain_api::CHAIN_GET_MESSAGE, Access::Read);
    access.insert(chain_api::CHAIN_READ_OBJ, Access::Read);
    access.insert(chain_api::CHAIN_HAS_OBJ, Access::Read);
    access.insert(chain_api::CHAIN_GET_BLOCK_MESSAGES, Access::Read);
    access.insert(chain_api::CHAIN_GET_TIPSET_BY_HEIGHT, Access::Read);
    access.insert(chain_api::CHAIN_GET_GENESIS, Access::Read);
    access.insert(chain_api::CHAIN_HEAD, Access::Read);
    access.insert(chain_api::CHAIN_HEAD_SUBSCRIPTION, Access::Read);
    access.insert(chain_api::CHAIN_NOTIFY, Access::Read);
    access.insert(chain_api::CHAIN_TIPSET_WEIGHT, Access::Read);
    access.insert(chain_api::CHAIN_GET_BLOCK, Access::Read);
    access.insert(chain_api::CHAIN_GET_TIPSET, Access::Read);
    access.insert(chain_api::CHAIN_GET_RANDOMNESS_FROM_TICKETS, Access::Read);
    access.insert(chain_api::CHAIN_GET_RANDOMNESS_FROM_BEACON, Access::Read);

    access
});

/// Checks an access enum against provided JWT claims
pub fn check_access(access: &Access, claims: &[String]) -> bool {
    match access {
        Access::Admin => claims.contains(&"admin".to_owned()),
        Access::Sign => claims.contains(&"sign".to_owned()),
        Access::Write => claims.contains(&"write".to_owned()),
        Access::Read => claims.contains(&"read".to_owned()),
    }
}

/// JSON-RPC API definitions

/// Auth API
pub mod auth_api {
    pub const AUTH_NEW: &str = "Filecoin.AuthNew";
    pub type AuthNewParams = (Vec<String>,);
    pub type AuthNewResult = Vec<u8>;

    pub const AUTH_VERIFY: &str = "Filecoin.AuthVerify";
    pub type AuthVerifyParams = (String,);
    pub type AuthVerifyResult = Vec<String>;
}

/// Beacon API
pub mod beacon_api {
    use beacon::json::BeaconEntryJson;
    use clock::ChainEpoch;

    pub const BEACON_GET_ENTRY: &str = "Filecoin.BeaconGetEntry";
    pub type BeaconGetEntryParams = (ChainEpoch,);
    pub type BeaconGetEntryResult = BeaconEntryJson;
}

/// Chain API
pub mod chain_api {
    use blocks::{
        header::json::BlockHeaderJson, tipset_json::TipsetJson, tipset_keys_json::TipsetKeysJson,
        TipsetKeys,
    };
    use chain::headchange_json::SubscriptionHeadChange;
    use cid::json::CidJson;
    use clock::ChainEpoch;
    use message::{unsigned_message::json::UnsignedMessageJson, BlockMessages};

    pub const CHAIN_GET_MESSAGE: &str = "Filecoin.ChainGetMessage";
    pub type ChainGetMessageParams = (CidJson,);
    pub type ChainGetMessageResult = UnsignedMessageJson;

    pub const CHAIN_READ_OBJ: &str = "Filecoin.ChainReadObj";
    pub type ChainReadObjParams = (CidJson,);
    pub type ChainReadObjResult = String;

    pub const CHAIN_HAS_OBJ: &str = "Filecoin.ChainHasObj";
    pub type ChainHasObjParams = (CidJson,);
    pub type ChainHasObjResult = bool;

    pub const CHAIN_GET_BLOCK_MESSAGES: &str = "Filecoin.ChainGetBlockMessages";
    pub type ChainGetBlockMessagesParams = (CidJson,);
    pub type ChainGetBlockMessagesResult = BlockMessages;

    pub const CHAIN_GET_TIPSET_BY_HEIGHT: &str = "Filecoin.ChainGetTipsetByHeight";
    pub type ChainGetTipsetByHeightParams = (ChainEpoch, TipsetKeys);
    pub type ChainGetTipsetByHeightResult = TipsetJson;

    pub const CHAIN_GET_GENESIS: &str = "Filecoin.ChainGetGenesis";
    pub type ChainGetGenesisParams = ();
    pub type ChainGetGenesisResult = Option<TipsetJson>;

    pub const CHAIN_HEAD: &str = "Filecoin.ChainHead";
    pub type ChainHeadParams = ();
    pub type ChainHeadResult = TipsetJson;

    pub const CHAIN_HEAD_SUBSCRIPTION: &str = "Filecoin.ChainHeadSubscription";
    pub type ChainHeadSubscriptionParams = ();
    pub type ChainHeadSubscriptionResult = i64;

    pub const CHAIN_NOTIFY: &str = "Filecoin.ChainNotify";
    pub type ChainNotifyParams = ();
    pub type ChainNotifyResult = SubscriptionHeadChange;

    pub const CHAIN_TIPSET_WEIGHT: &str = "Filecoin.ChainTipSetWeight";
    pub type ChainTipSetWeightParams = (TipsetKeysJson,);
    pub type ChainTipSetWeightResult = String;

    pub const CHAIN_GET_BLOCK: &str = "Filecoin.ChainGetBlock";
    pub type ChainGetBlockParams = (CidJson,);
    pub type ChainGetBlockResult = BlockHeaderJson;

    pub const CHAIN_GET_TIPSET: &str = "Filecoin.ChainGetTipSet";
    pub type ChainGetTipSetParams = (TipsetKeysJson,);
    pub type ChainGetTipSetResult = TipsetJson;

    pub const CHAIN_GET_RANDOMNESS_FROM_TICKETS: &str = "Filecoin.ChainGetRandomnessFromTickets";
    pub type ChainGetRandomnessFromTicketsParams =
        (TipsetKeysJson, i64, ChainEpoch, Option<String>);
    pub type ChainGetRandomnessFromTicketsResult = [u8; 32];

    pub const CHAIN_GET_RANDOMNESS_FROM_BEACON: &str = "Filecoin.ChainGetRandomnessFromBeacon";
    pub type ChainGetRandomnessFromBeaconParams = (TipsetKeysJson, i64, ChainEpoch, Option<String>);
    pub type ChainGetRandomnessFromBeaconResult = [u8; 32];
}

/// Message Pool API
pub mod mpool_api {
    use blocks::{tipset_keys_json::TipsetKeysJson, TipsetKeys};
    use cid::json::CidJson;
    use message::{
        signed_message::json::SignedMessageJson, unsigned_message::json::UnsignedMessageJson,
        MessageSendSpec,
    };

    pub const MPOOL_ESTIMATE_GAS_PRICE: &str = "Filecoin.MpoolEstimateGasPrice";
    pub type MpoolEstimateGasPriceParams = (u64, String, u64, TipsetKeys);
    pub type MpoolEstimateGasPriceResult = String;

    pub const MPOOL_GET_NONCE: &str = "Filecoin.MpoolGetNonce";
    pub type MpoolGetNonceParams = (String,);
    pub type MpoolGetNonceResult = u64;

    use cid::json::vec::CidJsonVec;
    use message::SignedMessage;

    pub const MPOOL_PENDING: &str = "Filecoin.MpoolPending";
    pub type MpoolPendingParams = (CidJsonVec,);
    pub type MpoolPendingResult = Vec<SignedMessage>;

    pub const MPOOL_PUSH: &str = "Filecoin.MpoolPush";
    pub type MpoolPushParams = (SignedMessageJson,);
    pub type MpoolPushResult = CidJson;

    pub const MPOOL_PUSH_MESSAGE: &str = "Filecoin.MpoolPushMessage";
    pub type MpoolPushMessageParams = (UnsignedMessageJson, Option<MessageSendSpec>);
    pub type MpoolPushMessageResult = SignedMessageJson;

    pub const MPOOL_SELECT: &str = "Filecoin.MpoolSelect";
    pub type MpoolSelectParams = (TipsetKeysJson, f64);
    pub type MpoolSelectResult = Vec<SignedMessageJson>;
}

/// Sync API
pub mod sync_check_bad {
    pub const SYNC_CHECK_BAD: &str = "Filecoin.SyncCheckBad";
    pub type SyncCheckBadParams = ();
    pub type SyncCheckBadResult = ();
}

pub mod sync_mark_bad {
    pub const SYNC_MARK_BAD: &str = "Filecoin.SyncMarkBad";
    pub type SyncMarkBadParams = ();
    pub type SyncMarkBadResult = ();
}

pub mod sync_state {
    pub const SYNC_STATE: &str = "Filecoin.SyncState";
    pub type SyncStateParams = ();
    pub type SyncStateResult = ();
}

pub mod sync_submit_block {
    pub const SYNC_SUBMIT_BLOCK: &str = "Filecoin.SyncSubmitBlock";
    pub type SyncSubmitBlockParams = ();
    pub type SyncSubmitBlockResult = ();
}

/// Wallet API
pub mod wallet_balance {
    pub const WALLET_BALANCE: &str = "Filecoin.WalletBalance";
    pub type WalletBalanceParams = ();
    pub type WalletBalanceResult = ();
}

pub mod wallet_default_address {
    pub const WALLET_DEFAULT_ADDRESS: &str = "Filecoin.WalletDefaultAddress";
    pub type WalletDefaultAddressParams = ();
    pub type WalletDefaultAddressResult = ();
}

pub mod wallet_export {
    pub const WALLET_EXPORT: &str = "Filecoin.WalletExport";
    pub type WalletExportParams = ();
    pub type WalletExportResult = ();
}

pub mod wallet_has {
    pub const WALLET_HAS: &str = "Filecoin.WalletHas";
    pub type WalletHasParams = ();
    pub type WalletHasResult = ();
}

pub mod wallet_import {
    pub const WALLET_IMPORT: &str = "Filecoin.WalletImport";
    pub type WalletImportParams = ();
    pub type WalletImportResult = ();
}

pub mod wallet_list {
    pub const WALLET_LIST: &str = "Filecoin.WalletList";
    pub type WalletListParams = ();
    pub type WalletListResult = ();
}

pub mod wallet_new {
    pub const WALLET_NEW: &str = "Filecoin.WalletNew";
    pub type WalletNewParams = ();
    pub type WalletNewResult = ();
}

pub mod wallet_set_default {
    pub const WALLET_SET_DEFAULT: &str = "Filecoin.WalletSetDefault";
    pub type WalletSetDefaultParams = ();
    pub type WalletSetDefaultResult = ();
}

pub mod wallet_sign {
    pub const WALLET_SIGN: &str = "Filecoin.WalletSign";
    pub type WalletSignParams = ();
    pub type WalletSignResult = ();
}

pub mod wallet_sign_message {
    pub const WALLET_SIGN_MESSAGE: &str = "Filecoin.WalletSignMessage";
    pub type WalletSignMessageParams = ();
    pub type WalletSignMessageResult = ();
}

pub mod wallet_verify {
    pub const WALLET_VERIFY: &str = "Filecoin.WalletVerify";
    pub type WalletVerifyParams = ();
    pub type WalletVerifyResult = ();
}

/// State API
pub mod state_miner_sectors {
    pub const STATE_MINER_SECTORS: &str = "Filecoin.StateMinerSectors";
    pub type StateMinerSectorsParams = ();
    pub type StateMinerSectorsResult = ();
}

pub mod state_call {
    pub const STATE_CALL: &str = "Filecoin.StateCall";
    pub type StateCallParams = ();
    pub type StateCallResult = ();
}

pub mod state_miner_deadlines {
    pub const STATE_MINER_DEADLINES: &str = "Filecoin.StateMinerDeadlines";
    pub type StateMinerDeadlinesParams = ();
    pub type StateMinerDeadlinesResult = ();
}

pub mod state_sector_precommit_info {
    pub const STATE_SECTOR_PRECOMMIT_INFO: &str = "Filecoin.StateSectorPrecommitInfo";
    pub type StateSectorPrecommitInfoParams = ();
    pub type StateSectorPrecommitInfoResult = ();
}

pub mod state_sector_get_info {
    pub const STATE_SECTOR_GET_INFO: &str = "Filecoin.StateSectorGetInfo";
    pub type StateSectorGetInfoParams = ();
    pub type StateSectorGetInfoResult = ();
}

pub mod state_miner_proving_deadline {
    pub const STATE_MINER_PROVING_DEADLINE: &str = "Filecoin.StateMinerProvingDeadline";
    pub type StateMinerProvingDeadlineParams = ();
    pub type StateMinerProvingDeadlineResult = ();
}

pub mod state_miner_info {
    pub const STATE_MINER_INFO: &str = "Filecoin.StateMinerInfo";
    pub type StateMinerInfoParams = ();
    pub type StateMinerInfoResult = ();
}

pub mod state_miner_faults {
    pub const STATE_MINER_FAULTS: &str = "Filecoin.StateMinerFaults";
    pub type StateMinerFaultsParams = ();
    pub type StateMinerFaultsResult = ();
}

pub mod state_all_miner_faults {
    pub const STATE_ALL_MINER_FAULTS: &str = "Filecoin.StateAllMinerFaults";
    pub type StateAllMinerFaultsParams = ();
    pub type StateAllMinerFaultsResult = ();
}

pub mod state_miner_recoveries {
    pub const STATE_MINER_RECOVERIES: &str = "Filecoin.StateMinerRecoveries";
    pub type StateMinerRecoveriesParams = ();
    pub type StateMinerRecoveriesResult = ();
}

pub mod state_miner_partitions {
    pub const STATE_MINER_PARTITIONS: &str = "Filecoin.StateMinerPartitions";
    pub type StateMinerPartitionsParams = ();
    pub type StateMinerPartitionsResult = ();
}

pub mod state_miner_pre_commit_deposit_for_power {
    pub const STATE_MINER_PRE_COMMIT_DEPOSIT_FOR_POWER: &str =
        "Filecoin.StateMinerPreCommitDepositForPower";
    pub type StateMinerPreCommitDepositForPowerParams = ();
    pub type StateMinerPreCommitDepositForPowerResult = ();
}

pub mod state_miner_initial_pledge_collateral {
    pub const STATE_MINER_INITIAL_PLEDGE_COLLATERAL: &str =
        "Filecoin.StateMinerInitialPledgeCollateral";
    pub type StateMinerInitialPledgeCollateralParams = ();
    pub type StateMinerInitialPledgeCollateralResult = ();
}

pub mod state_replay {
    pub const STATE_REPLAY: &str = "Filecoin.StateReplay";
    pub type StateReplayParams = ();
    pub type StateReplayResult = ();
}

pub mod state_get_actor {
    pub const STATE_GET_ACTOR: &str = "Filecoin.StateGetActor";
    pub type StateGetActorParams = ();
    pub type StateGetActorResult = ();
}

pub mod state_account_key {
    pub const STATE_ACCOUNT_KEY: &str = "Filecoin.StateAccountKey";
    pub type StateAccountKeyParams = ();
    pub type StateAccountKeyResult = ();
}

pub mod state_lookup_id {
    pub const STATE_LOOKUP_ID: &str = "Filecoin.StateLookupId";
    pub type StateLookupIdParams = ();
    pub type StateLookupIdResult = ();
}

pub mod state_market_balance {
    pub const STATE_MARKET_BALANCE: &str = "Filecoin.StateMarketBalance";
    pub type StateMarketBalanceParams = ();
    pub type StateMarketBalanceResult = ();
}

pub mod state_market_deals {
    pub const STATE_MARKET_DEALS: &str = "Filecoin.StateMarketDeals";
    pub type StateMarketDealsParams = ();
    pub type StateMarketDealsResult = ();
}

pub mod state_get_receipt {
    pub const STATE_GET_RECEIPT: &str = "Filecoin.StateGetReceipt";
    pub type StateGetReceiptParams = ();
    pub type StateGetReceiptResult = ();
}

pub mod state_wait_msg {
    pub const STATE_WAIT_MSG: &str = "Filecoin.StateWaitMsg";
    pub type StateWaitMsgParams = ();
    pub type StateWaitMsgResult = ();
}

pub mod state_miner_sector_allocated {
    pub const STATE_MINER_SECTOR_ALLOCATED: &str = "Filecoin.StateMinerSectorAllocated";
    pub type StateMinerSectorAllocatedParams = ();
    pub type StateMinerSectorAllocatedResult = ();
}

pub mod state_network_name {
    pub const STATE_NETWORK_NAME: &str = "Filecoin.StateNetworkName";
    pub type StateNetworkNameParams = ();
    pub type StateNetworkNameResult = ();
}

pub mod miner_get_base_info {
    pub const MINER_GET_BASE_INFO: &str = "Filecoin.MinerGetBaseInfo";
    pub type MinerGetBaseInfoParams = ();
    pub type MinerGetBaseInfoResult = ();
}

pub mod miner_create_block {
    pub const MINER_CREATE_BLOCK: &str = "Filecoin.MinerCreateBlock";
    pub type MinerCreateBlockParams = ();
    pub type MinerCreateBlockResult = ();
}

pub mod state_network_version {
    pub const STATE_NETWORK_VERSION: &str = "Filecoin.StateNetworkVersion";
    pub type StateNetworkVersionParams = ();
    pub type StateNetworkVersionResult = ();
}

/// Gas API
pub mod gas_estimate_gas_limit {
    pub const GAS_ESTIMATE_GAS_LIMIT: &str = "Filecoin.GasEstimateGasLimit";
    pub type GasEstimateGasLimitParams = ();
    pub type GasEstimateGasLimitResult = ();
}

pub mod gas_estimate_gas_premium {
    pub const GAS_ESTIMATE_GAS_PREMIUM: &str = "Filecoin.GasEstimateGasPremium";
    pub type GasEstimateGasPremiumParams = ();
    pub type GasEstimateGasPremiumResult = ();
}

pub mod gas_estimate_fee_cap {
    pub const GAS_ESTIMATE_FEE_CAP: &str = "Filecoin.GasEstimateFeeCap";
    pub type GasEstimateFeeCapParams = ();
    pub type GasEstimateFeeCapResult = ();
}

pub mod gas_estimate_message_gas {
    pub const GAS_ESTIMATE_MESSAGE_GAS: &str = "Filecoin.GasEstimateMessageGas";
    pub type GasEstimateMessageGasParams = ();
    pub type GasEstimateMessageGasResult = ();
}

/// Common API
pub mod version {
    pub const VERSION: &str = "Filecoin.Version";
    pub type VersionParams = ();
    pub type VersionResult = ();
}

/// Net API
pub mod net_addrs_listen {
    pub const NET_ADDRS_LISTEN: &str = "Filecoin.NetAddrsListen";
    pub type NetAddrsListenParams = ();
    pub type NetAddrsListenResult = ();
}
