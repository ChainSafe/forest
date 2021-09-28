// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use once_cell::sync::Lazy;
use std::collections::HashMap;

pub mod data_types;

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

    // Auth API
    access.insert(auth_api::AUTH_NEW, Access::Admin);
    access.insert(auth_api::AUTH_VERIFY, Access::Read);

    // Beacon API
    access.insert(beacon_api::BEACON_GET_ENTRY, Access::Read);

    // Chain API
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

    // Message Pool API
    access.insert(mpool_api::MPOOL_ESTIMATE_GAS_PRICE, Access::Read);
    access.insert(mpool_api::MPOOL_GET_NONCE, Access::Read);
    access.insert(mpool_api::MPOOL_PENDING, Access::Read);
    access.insert(mpool_api::MPOOL_PUSH, Access::Write);
    access.insert(mpool_api::MPOOL_PUSH_MESSAGE, Access::Sign);
    access.insert(mpool_api::MPOOL_SELECT, Access::Read);

    // Sync API
    access.insert(sync_api::SYNC_CHECK_BAD, Access::Read);
    access.insert(sync_api::SYNC_MARK_BAD, Access::Admin);
    access.insert(sync_api::SYNC_STATE, Access::Read);
    access.insert(sync_api::SYNC_SUBMIT_BLOCK, Access::Write);

    // Wallet API
    access.insert(wallet_api::WALLET_BALANCE, Access::Write);
    access.insert(wallet_api::WALLET_DEFAULT_ADDRESS, Access::Write);
    access.insert(wallet_api::WALLET_EXPORT, Access::Admin);
    access.insert(wallet_api::WALLET_HAS, Access::Write);
    access.insert(wallet_api::WALLET_IMPORT, Access::Admin);
    access.insert(wallet_api::WALLET_LIST, Access::Write);
    access.insert(wallet_api::WALLET_NEW, Access::Write);
    access.insert(wallet_api::WALLET_SET_DEFAULT, Access::Write);
    access.insert(wallet_api::WALLET_SIGN, Access::Sign);
    access.insert(wallet_api::WALLET_SIGN_MESSAGE, Access::Sign);
    access.insert(wallet_api::WALLET_VERIFY, Access::Read);

    // State API
    access.insert(state_api::STATE_MINER_SECTORS, Access::Read);
    access.insert(state_api::STATE_CALL, Access::Read);
    access.insert(state_api::STATE_MINER_DEADLINES, Access::Read);
    access.insert(state_api::STATE_SECTOR_PRECOMMIT_INFO, Access::Read);
    access.insert(state_api::STATE_SECTOR_GET_INFO, Access::Read);
    access.insert(state_api::STATE_MINER_PROVING_DEADLINE, Access::Read);
    access.insert(state_api::STATE_MINER_INFO, Access::Read);
    access.insert(state_api::STATE_MINER_FAULTS, Access::Read);
    access.insert(state_api::STATE_ALL_MINER_FAULTS, Access::Read);
    access.insert(state_api::STATE_MINER_RECOVERIES, Access::Read);
    access.insert(state_api::STATE_MINER_PARTITIONS, Access::Read);
    access.insert(
        state_api::STATE_MINER_PRE_COMMIT_DEPOSIT_FOR_POWER,
        Access::Read,
    );
    access.insert(
        state_api::STATE_MINER_INITIAL_PLEDGE_COLLATERAL,
        Access::Read,
    );
    access.insert(state_api::STATE_REPLAY, Access::Read);
    access.insert(state_api::STATE_GET_ACTOR, Access::Read);
    access.insert(state_api::STATE_ACCOUNT_KEY, Access::Read);
    access.insert(state_api::STATE_LOOKUP_ID, Access::Read);
    access.insert(state_api::STATE_MARKET_BALANCE, Access::Read);
    access.insert(state_api::STATE_MARKET_DEALS, Access::Read);
    access.insert(state_api::STATE_GET_RECEIPT, Access::Read);
    access.insert(state_api::STATE_WAIT_MSG, Access::Read);
    access.insert(state_api::STATE_MINER_SECTOR_ALLOCATED, Access::Read);
    access.insert(state_api::STATE_NETWORK_NAME, Access::Read);
    access.insert(state_api::MINER_GET_BASE_INFO, Access::Read);
    access.insert(state_api::MINER_CREATE_BLOCK, Access::Write);
    access.insert(state_api::STATE_NETWORK_VERSION, Access::Read);

    // Gas API
    access.insert(gas_api::GAS_ESTIMATE_GAS_LIMIT, Access::Read);
    access.insert(gas_api::GAS_ESTIMATE_GAS_PREMIUM, Access::Read);
    access.insert(gas_api::GAS_ESTIMATE_FEE_CAP, Access::Read);
    access.insert(gas_api::GAS_ESTIMATE_MESSAGE_GAS, Access::Read);

    // Common API
    access.insert(common_api::VERSION, Access::Read);

    // Net API
    access.insert(net_api::NET_ADDRS_LISTEN, Access::Read);
    access.insert(net_api::NET_PEERS, Access::Read);
    access.insert(net_api::NET_CONNECT, Access::Write);
    access.insert(net_api::NET_DISCONNECT, Access::Write);

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

/// JSON-RPC API defaults
pub const DEFAULT_MULTIADDRESS: &str = "/ip4/127.0.0.1/tcp/1234/http";
pub const API_INFO_KEY: &str = "FULLNODE_API_INFO";

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
    use crate::data_types::BlockMessages;
    use blocks::{
        header::json::BlockHeaderJson, tipset_json::TipsetJson, tipset_keys_json::TipsetKeysJson,
        TipsetKeys,
    };
    use chain::headchange_json::SubscriptionHeadChange;
    use cid::json::CidJson;
    use clock::ChainEpoch;
    use message::unsigned_message::json::UnsignedMessageJson;

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
    use crate::data_types::MessageSendSpec;
    use blocks::{tipset_keys_json::TipsetKeysJson, TipsetKeys};
    use cid::json::CidJson;
    use message::{
        signed_message::json::SignedMessageJson, unsigned_message::json::UnsignedMessageJson,
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
pub mod sync_api {
    use crate::data_types::RPCSyncState;
    use blocks::gossip_block::json::GossipBlockJson;
    use cid::json::CidJson;

    pub const SYNC_CHECK_BAD: &str = "Filecoin.SyncCheckBad";
    pub type SyncCheckBadParams = (CidJson,);
    pub type SyncCheckBadResult = String;

    pub const SYNC_MARK_BAD: &str = "Filecoin.SyncMarkBad";
    pub type SyncMarkBadParams = (CidJson,);
    pub type SyncMarkBadResult = ();

    pub const SYNC_STATE: &str = "Filecoin.SyncState";
    pub type SyncStateParams = ();
    pub type SyncStateResult = RPCSyncState;

    pub const SYNC_SUBMIT_BLOCK: &str = "Filecoin.SyncSubmitBlock";
    pub type SyncSubmitBlockParams = (GossipBlockJson,);
    pub type SyncSubmitBlockResult = ();
}

/// Wallet API
pub mod wallet_api {
    use address::json::AddressJson;
    use crypto::signature::json::{signature_type::SignatureTypeJson, SignatureJson};
    use message::{
        signed_message::json::SignedMessageJson, unsigned_message::json::UnsignedMessageJson,
    };
    use wallet::json::KeyInfoJson;

    pub const WALLET_BALANCE: &str = "Filecoin.WalletBalance";
    pub type WalletBalanceParams = (String,);
    pub type WalletBalanceResult = String;

    pub const WALLET_DEFAULT_ADDRESS: &str = "Filecoin.WalletDefaultAddress";
    pub type WalletDefaultAddressParams = ();
    pub type WalletDefaultAddressResult = String;

    pub const WALLET_EXPORT: &str = "Filecoin.WalletExport";
    pub type WalletExportParams = (String,);
    pub type WalletExportResult = KeyInfoJson;

    pub const WALLET_HAS: &str = "Filecoin.WalletHas";
    pub type WalletHasParams = (String,);
    pub type WalletHasResult = bool;

    pub const WALLET_IMPORT: &str = "Filecoin.WalletImport";
    pub type WalletImportParams = Vec<KeyInfoJson>;
    pub type WalletImportResult = String;

    pub const WALLET_LIST: &str = "Filecoin.WalletList";
    pub type WalletListParams = ();
    pub type WalletListResult = Vec<AddressJson>;

    pub const WALLET_NEW: &str = "Filecoin.WalletNew";
    pub type WalletNewParams = (SignatureTypeJson,);
    pub type WalletNewResult = String;

    pub const WALLET_SET_DEFAULT: &str = "Filecoin.WalletSetDefault";
    pub type WalletSetDefaultParams = (AddressJson,);
    pub type WalletSetDefaultResult = ();

    pub const WALLET_SIGN: &str = "Filecoin.WalletSign";
    pub type WalletSignParams = (AddressJson, Vec<u8>);
    pub type WalletSignResult = SignatureJson;

    pub const WALLET_SIGN_MESSAGE: &str = "Filecoin.WalletSignMessage";
    pub type WalletSignMessageParams = (String, UnsignedMessageJson);
    pub type WalletSignMessageResult = SignedMessageJson;

    pub const WALLET_VERIFY: &str = "Filecoin.WalletVerify";
    pub type WalletVerifyParams = (String, String, SignatureJson);
    pub type WalletVerifyResult = bool;
}

/// State API
pub mod state_api {
    use std::collections::HashMap;

    use crate::data_types::{
        ActorStateJson, BlockTemplate, Deadline, Fault, MarketDeal, MessageLookup,
        MiningBaseInfoJson, Partition,
    };
    use actor::miner::{
        MinerInfo, SectorOnChainInfo, SectorPreCommitInfo, SectorPreCommitOnChainInfo,
    };
    use address::{json::AddressJson, Address};
    use bitfield::json::BitFieldJson;
    use blocks::{
        gossip_block::json::GossipBlockJson as BlockMsgJson, tipset_keys_json::TipsetKeysJson,
    };
    use cid::json::CidJson;
    use clock::ChainEpoch;
    use fil_types::{deadlines::DeadlineInfo, NetworkVersion, SectorNumber};
    use message::{
        message_receipt::json::MessageReceiptJson, unsigned_message::json::UnsignedMessageJson,
    };
    use state_manager::{InvocResult, MarketBalance};

    pub const STATE_MINER_SECTORS: &str = "Filecoin.StateMinerSectors";
    pub type StateMinerSectorsParams = (AddressJson, BitFieldJson, TipsetKeysJson);
    pub type StateMinerSectorsResult = Vec<SectorOnChainInfo>;

    pub const STATE_CALL: &str = "Filecoin.StateCall";
    pub type StateCallParams = (UnsignedMessageJson, TipsetKeysJson);
    pub type StateCallResult = InvocResult;

    pub const STATE_MINER_DEADLINES: &str = "Filecoin.StateMinerDeadlines";
    pub type StateMinerDeadlinesParams = (AddressJson, TipsetKeysJson);
    pub type StateMinerDeadlinesResult = Vec<Deadline>;

    pub const STATE_SECTOR_PRECOMMIT_INFO: &str = "Filecoin.StateSectorPrecommitInfo";
    pub type StateSectorPrecommitInfoParams = (AddressJson, SectorNumber, TipsetKeysJson);
    pub type StateSectorPrecommitInfoResult = SectorPreCommitOnChainInfo;

    pub const STATE_MINER_INFO: &str = "Filecoin.StateMinerInfo";
    pub type StateMinerInfoParams = (AddressJson, TipsetKeysJson);
    pub type StateMinerInfoResult = MinerInfo;

    pub const STATE_SECTOR_GET_INFO: &str = "Filecoin.StateSectorGetInfo";
    pub type StateSectorGetInfoParams = (AddressJson, SectorNumber, TipsetKeysJson);
    pub type StateSectorGetInfoResult = Option<SectorOnChainInfo>;

    pub const STATE_MINER_PROVING_DEADLINE: &str = "Filecoin.StateMinerProvingDeadline";
    pub type StateMinerProvingDeadlineParams = (AddressJson, TipsetKeysJson);
    pub type StateMinerProvingDeadlineResult = DeadlineInfo;

    pub const STATE_MINER_FAULTS: &str = "Filecoin.StateMinerFaults";
    pub type StateMinerFaultsParams = (AddressJson, TipsetKeysJson);
    pub type StateMinerFaultsResult = BitFieldJson;

    pub const STATE_ALL_MINER_FAULTS: &str = "Filecoin.StateAllMinerFaults";
    pub type StateAllMinerFaultsParams = (ChainEpoch, TipsetKeysJson);
    pub type StateAllMinerFaultsResult = Vec<Fault>;

    pub const STATE_MINER_RECOVERIES: &str = "Filecoin.StateMinerRecoveries";
    pub type StateMinerRecoveriesParams = (AddressJson, TipsetKeysJson);
    pub type StateMinerRecoveriesResult = BitFieldJson;

    pub const STATE_MINER_PARTITIONS: &str = "Filecoin.StateMinerPartitions";
    pub type StateMinerPartitionsParams = (AddressJson, u64, TipsetKeysJson);
    pub type StateMinerPartitionsResult = Vec<Partition>;

    pub const STATE_REPLAY: &str = "Filecoin.StateReplay";
    pub type StateReplayParams = (CidJson, TipsetKeysJson);
    pub type StateReplayResult = InvocResult;

    pub const STATE_NETWORK_NAME: &str = "Filecoin.StateNetworkName";
    pub type StateNetworkNameParams = ();
    pub type StateNetworkNameResult = String;

    pub const STATE_NETWORK_VERSION: &str = "Filecoin.StateNetworkVersion";
    pub type StateNetworkVersionParams = (TipsetKeysJson,);
    pub type StateNetworkVersionResult = NetworkVersion;

    pub const STATE_GET_ACTOR: &str = "Filecoin.StateGetActor";
    pub type StateGetActorParams = (AddressJson, TipsetKeysJson);
    pub type StateGetActorResult = Option<ActorStateJson>;

    pub const STATE_ACCOUNT_KEY: &str = "Filecoin.StateAccountKey";
    pub type StateAccountKeyParams = (AddressJson, TipsetKeysJson);
    pub type StateAccountKeyResult = Option<AddressJson>;

    pub const STATE_LOOKUP_ID: &str = "Filecoin.StateLookupId";
    pub type StateLookupIdParams = (AddressJson, TipsetKeysJson);
    pub type StateLookupIdResult = Option<Address>;

    pub const STATE_MARKET_BALANCE: &str = "Filecoin.StateMarketBalance";
    pub type StateMarketBalanceParams = (AddressJson, TipsetKeysJson);
    pub type StateMarketBalanceResult = MarketBalance;

    pub const STATE_MARKET_DEALS: &str = "Filecoin.StateMarketDeals";
    pub type StateMarketDealsParams = (TipsetKeysJson,);
    pub type StateMarketDealsResult = HashMap<String, MarketDeal>;

    pub const STATE_GET_RECEIPT: &str = "Filecoin.StateGetReceipt";
    pub type StateGetReceiptParams = (CidJson, TipsetKeysJson);
    pub type StateGetReceiptResult = MessageReceiptJson;

    pub const STATE_WAIT_MSG: &str = "Filecoin.StateWaitMsg";
    pub type StateWaitMsgParams = (CidJson, i64);
    pub type StateWaitMsgResult = MessageLookup;

    pub const MINER_CREATE_BLOCK: &str = "Filecoin.MinerCreateBlock";
    pub type MinerCreateBlockParams = (BlockTemplate,);
    pub type MinerCreateBlockResult = BlockMsgJson;

    pub const STATE_MINER_SECTOR_ALLOCATED: &str = "Filecoin.StateMinerSectorAllocated";
    pub type StateMinerSectorAllocatedParams = (AddressJson, u64, TipsetKeysJson);
    pub type StateMinerSectorAllocatedResult = bool;

    pub const STATE_MINER_PRE_COMMIT_DEPOSIT_FOR_POWER: &str =
        "Filecoin.StateMinerPreCommitDepositForPower";
    pub type StateMinerPreCommitDepositForPowerParams =
        (AddressJson, SectorPreCommitInfo, TipsetKeysJson);
    pub type StateMinerPreCommitDepositForPowerResult = String;

    pub const STATE_MINER_INITIAL_PLEDGE_COLLATERAL: &str =
        "Filecoin.StateMinerInitialPledgeCollateral";
    pub type StateMinerInitialPledgeCollateralParams =
        (AddressJson, SectorPreCommitInfo, TipsetKeysJson);
    pub type StateMinerInitialPledgeCollateralResult = String;

    pub const MINER_GET_BASE_INFO: &str = "Filecoin.MinerGetBaseInfo";
    pub type MinerGetBaseInfoParams = (AddressJson, ChainEpoch, TipsetKeysJson);
    pub type MinerGetBaseInfoResult = Option<MiningBaseInfoJson>;
}

/// Gas API
pub mod gas_api {
    use crate::data_types::MessageSendSpec;
    use address::json::AddressJson;
    use blocks::tipset_keys_json::TipsetKeysJson;
    use message::unsigned_message::json::UnsignedMessageJson;

    pub const GAS_ESTIMATE_FEE_CAP: &str = "Filecoin.GasEstimateFeeCap";
    pub type GasEstimateFeeCapParams = (UnsignedMessageJson, i64, TipsetKeysJson);
    pub type GasEstimateFeeCapResult = String;

    pub const GAS_ESTIMATE_GAS_PREMIUM: &str = "Filecoin.GasEstimateGasPremium";
    pub type GasEstimateGasPremiumParams = (u64, AddressJson, i64, TipsetKeysJson);
    pub type GasEstimateGasPremiumResult = String;

    pub const GAS_ESTIMATE_GAS_LIMIT: &str = "Filecoin.GasEstimateGasLimit";
    pub type GasEstimateGasLimitParams = (UnsignedMessageJson, TipsetKeysJson);
    pub type GasEstimateGasLimitResult = i64;

    pub const GAS_ESTIMATE_MESSAGE_GAS: &str = "Filecoin.GasEstimateMessageGas";
    pub type GasEstimateMessageGasParams =
        (UnsignedMessageJson, Option<MessageSendSpec>, TipsetKeysJson);
    pub type GasEstimateMessageGasResult = UnsignedMessageJson;
}

/// Common API
pub mod common_api {
    use fil_types::build_version::APIVersion;

    pub const VERSION: &str = "Filecoin.Version";
    pub type VersionParams = ();
    pub type VersionResult = APIVersion;
}

/// Net API
pub mod net_api {
    use crate::data_types::AddrInfo;

    pub const NET_ADDRS_LISTEN: &str = "Filecoin.NetAddrsListen";
    pub type NetAddrsListenParams = ();
    pub type NetAddrsListenResult = AddrInfo;

    pub const NET_PEERS: &str = "Filecoin.NetPeers";
    pub type NetPeersParams = ();
    pub type NetPeersResult = Vec<AddrInfo>;

    pub const NET_CONNECT: &str = "Filecoin.NetConnect";
    pub type NetConnectParams = (AddrInfo,);
    pub type NetConnectResult = ();

    pub const NET_DISCONNECT: &str = "Filecoin.NetDisconnect";
    pub type NetDisconnectParams = (String,);
    pub type NetDisconnectResult = ();
}
