pub mod archive;
pub mod historical;
pub mod store;
pub use historical::HistoricalSnapshot;
pub mod forest;

pub type ChainEpoch = u64;
pub type ChainEpochDelta = u64;
// const FOREST_PROJECT: &str = "forest-391213";
pub const FOREST_PROJECT: &str = "sturdy-willow-391212";

pub const R2_ENDPOINT: &str =
    "https://2238a825c5aca59233eab1f221f7aefb.r2.cloudflarestorage.com/forest-archive";

pub const EPOCH_STEP: ChainEpochDelta = 30_000;
pub const DIFF_STEP: ChainEpochDelta = 3_000;

pub const MAINNET_GENESIS_TIMESTAMP: u64 = 1598306400;
pub const EPOCH_DURATION_SECONDS: u64 = 30;
