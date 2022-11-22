use cid::Cid;
use fil_actors_runtime_v9::BatchReturn;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::bigint::{bigint_ser, BigInt};
use fvm_shared::clock::ChainEpoch;
use fvm_shared::deal::DealID;
use fvm_shared::econ::TokenAmount;
use fvm_shared::piece::PaddedPieceSize;
use fvm_shared::sector::SectorNumber;
use fvm_shared::sector::{RegisteredSealProof, StoragePower};
use fvm_shared::smooth::FilterEstimate;
use fvm_shared::ActorID;

pub mod account {
    pub const PUBKEY_ADDRESS_METHOD: u64 = 2;
}

pub mod market {

    use super::*;

    pub const VERIFY_DEALS_FOR_ACTIVATION_METHOD: u64 = 5;
    pub const ACTIVATE_DEALS_METHOD: u64 = 6;
    pub const ON_MINER_SECTORS_TERMINATE_METHOD: u64 = 7;
    pub const COMPUTE_DATA_COMMITMENT_METHOD: u64 = 8;

    #[derive(Serialize_tuple, Deserialize_tuple)]
    pub struct SectorDeals {
        pub sector_type: RegisteredSealProof,
        pub sector_expiry: ChainEpoch,
        pub deal_ids: Vec<DealID>,
    }

    #[derive(Serialize_tuple, Deserialize_tuple)]
    pub struct ActivateDealsParams {
        pub deal_ids: Vec<DealID>,
        pub sector_expiry: ChainEpoch,
    }

    #[derive(Serialize_tuple, Deserialize_tuple, Clone)]
    pub struct VerifiedDealInfo {
        pub client: ActorID,
        pub allocation_id: u64,
        pub data: Cid,
        pub size: PaddedPieceSize,
    }

    impl Default for VerifiedDealInfo {
        fn default() -> VerifiedDealInfo {
            VerifiedDealInfo {
                size: PaddedPieceSize(0),
                client: 0,
                allocation_id: 0,
                data: Default::default(),
            }
        }
    }

    #[derive(Serialize_tuple, Deserialize_tuple)]
    pub struct ActivateDealsResult {
        #[serde(with = "bigint_ser")]
        pub nonverified_deal_space: BigInt,
        pub verified_infos: Vec<VerifiedDealInfo>,
    }

    #[derive(Serialize_tuple, Deserialize_tuple, Clone, Default)]
    pub struct DealSpaces {
        #[serde(with = "bigint_ser")]
        pub deal_space: BigInt,
        #[serde(with = "bigint_ser")]
        pub verified_deal_space: BigInt,
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

    #[derive(Serialize_tuple, Deserialize_tuple, Default, Clone)]
    pub struct SectorDealData {
        /// Option::None signifies commitment to empty sector, meaning no deals.
        pub commd: Option<Cid>,
    }

    #[derive(Serialize_tuple, Deserialize_tuple, Default, Clone)]
    pub struct VerifyDealsForActivationReturn {
        pub sectors: Vec<SectorDealData>,
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

pub mod verifreg {
    use super::*;

    pub const GET_CLAIMS_METHOD: u64 = 10;
    pub const CLAIM_ALLOCATIONS_METHOD: u64 = 9;

    pub type ClaimID = u64;
    pub type AllocationID = u64;

    #[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug, PartialEq, Eq)]
    pub struct Claim {
        // The provider storing the data (from allocation).
        pub provider: ActorID,
        // The client which allocated the DataCap (from allocation).
        pub client: ActorID,
        // Identifier of the data committed (from allocation).
        pub data: Cid,
        // The (padded) size of data (from allocation).
        pub size: PaddedPieceSize,
        // The min period which the provider must commit to storing data
        pub term_min: ChainEpoch,
        // The max period for which provider can earn QA-power for the data
        pub term_max: ChainEpoch,
        // The epoch at which the (first range of the) piece was committed.
        pub term_start: ChainEpoch,
        // ID of the provider's sector in which the data is committed.
        pub sector: SectorNumber,
    }
    #[derive(Debug, Serialize_tuple, Deserialize_tuple)]
    pub struct GetClaimsParams {
        pub provider: ActorID,
        pub claim_ids: Vec<ClaimID>,
    }
    #[derive(Debug, Serialize_tuple, Deserialize_tuple)]

    pub struct GetClaimsReturn {
        pub batch_info: BatchReturn,
        pub claims: Vec<Claim>,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
    pub struct SectorAllocationClaim {
        pub client: ActorID,
        pub allocation_id: AllocationID,
        pub data: Cid,
        pub size: PaddedPieceSize,
        pub sector: SectorNumber,
        pub sector_expiry: ChainEpoch,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
    pub struct ClaimAllocationsParams {
        pub sectors: Vec<SectorAllocationClaim>,
        pub all_or_nothing: bool,
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
    pub struct ClaimAllocationsReturn {
        pub batch_info: BatchReturn,
        #[serde(with = "bigint_ser")]
        pub claimed_space: BigInt,
    }
}
