use fvm_ipld_encoding::tuple::*;
use fvm_shared::econ::TokenAmount;

pub mod miner {
    use super::*;

    pub const APPLY_REWARDS_METHOD: u64 = 14;

    #[derive(Debug, Serialize_tuple, Deserialize_tuple)]
    pub struct ApplyRewardParams {
        pub reward: TokenAmount,
        pub penalty: TokenAmount,
    }
}
