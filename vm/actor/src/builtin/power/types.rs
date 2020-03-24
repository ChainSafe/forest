use crate::StoragePower;
use num_bigint::BigInt;

lazy_static! {
    /// Minimum number of registered miners for the minimum miner size limit to effectively limit consensus power.
    pub static ref CONSENSUS_MINER_MIN_POWER: StoragePower = BigInt::from(2 << 30);
}
