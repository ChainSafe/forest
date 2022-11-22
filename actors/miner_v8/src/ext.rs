use cid::Cid;
use fil_actors_runtime_v8::DealWeight;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::bigint::bigint_ser;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::deal::DealID;
use fvm_shared::econ::TokenAmount;
use fvm_shared::sector::{RegisteredSealProof, StoragePower};
use fvm_shared::smooth::FilterEstimate;

pub mod account {
    pub const PUBKEY_ADDRESS_METHOD: u64 = 2;
}

pub mod market {

    use super::*;

    pub const VERIFY_DEALS_FOR_ACTIVATION_METHOD: u64 = 5;
    pub const ACTIVATE_DEALS_METHOD: u64 = 6;
    pub const ON_MINER_SECTORS_TERMINATE_METHOD: u64 = 7;
    pub const COMPUTE_DATA_COMMITMENT_METHOD: u64 = 8;

    #[derive(Serialize_tuple, Deserialize_tuple, Default)]
    pub struct SectorWeights {
        pub deal_space: u64,
        #[serde(with = "bigint_ser")]
        pub deal_weight: DealWeight,
        #[serde(with = "bigint_ser")]
        pub verified_deal_weight: DealWeight,
    }

    #[derive(Serialize_tuple, Deserialize_tuple)]
    pub struct SectorDeals {
        pub sector_expiry: ChainEpoch,
        pub deal_ids: Vec<DealID>,
    }

    #[derive(Serialize_tuple, Deserialize_tuple)]
    pub struct ActivateDealsParams {
        pub deal_ids: Vec<DealID>,
        pub sector_expiry: ChainEpoch,
    }

    #[derive(Serialize_tuple)]
    pub struct ComputeDataCommitmentParamsRef<'a> {
        pub inputs: &'a [SectorDataSpec],
    }

    #[derive(Serialize_tuple, Deserialize_tuple)]
    pub struct ComputeDataCommitmentReturn {
        pub commds: Vec<Cid>,
    }

    #[derive(Serialize_tuple, Deserialize_tuple)]
    pub struct OnMinerSectorsTerminateParams {
        pub epoch: ChainEpoch,
        pub deal_ids: Vec<DealID>,
    }

    #[derive(Serialize_tuple)]
    pub struct OnMinerSectorsTerminateParamsRef<'a> {
        pub epoch: ChainEpoch,
        pub deal_ids: &'a [DealID],
    }

    #[derive(Serialize_tuple, Deserialize_tuple)]
    pub struct SectorDataSpec {
        pub deal_ids: Vec<DealID>,
        pub sector_type: RegisteredSealProof,
    }

    #[derive(Serialize_tuple)]
    pub struct VerifyDealsForActivationParamsRef<'a> {
        pub sectors: &'a [SectorDeals],
    }

    #[derive(Serialize_tuple, Deserialize_tuple, Default)]
    pub struct VerifyDealsForActivationReturn {
        pub sectors: Vec<SectorWeights>,
    }
}

pub mod power {
    use super::*;

    pub const UPDATE_CLAIMED_POWER_METHOD: u64 = 3;
    pub const ENROLL_CRON_EVENT_METHOD: u64 = 4;
    pub const UPDATE_PLEDGE_TOTAL_METHOD: u64 = 6;
    pub const SUBMIT_POREP_FOR_BULK_VERIFY_METHOD: u64 = 8;
    pub const CURRENT_TOTAL_POWER_METHOD: u64 = 9;

    #[derive(Serialize_tuple, Deserialize_tuple)]
    pub struct CurrentTotalPowerReturn {
        #[serde(with = "bigint_ser")]
        pub raw_byte_power: StoragePower,
        #[serde(with = "bigint_ser")]
        pub quality_adj_power: StoragePower,
        pub pledge_collateral: TokenAmount,
        pub quality_adj_power_smoothed: FilterEstimate,
    }
    #[derive(Serialize_tuple, Deserialize_tuple)]
    pub struct EnrollCronEventParams {
        pub event_epoch: ChainEpoch,
        pub payload: RawBytes,
    }

    #[derive(Serialize_tuple, Deserialize_tuple)]
    pub struct UpdateClaimedPowerParams {
        #[serde(with = "bigint_ser")]
        pub raw_byte_delta: StoragePower,
        #[serde(with = "bigint_ser")]
        pub quality_adjusted_delta: StoragePower,
    }

    pub const MAX_MINER_PROVE_COMMITS_PER_EPOCH: usize = 200;
}

pub mod reward {
    pub const THIS_EPOCH_REWARD_METHOD: u64 = 3;
}
