use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::tuple::{Deserialize_tuple, Serialize_tuple};
use fvm_shared::address::Address;

pub mod datacap {
    use super::*;
    use fvm_shared::econ::TokenAmount;

    #[repr(u64)]
    pub enum Method {
        // Non-standard.
        Mint = 2,
        Destroy = 3,
        // Static method numbers for token standard methods, for private use.
        // Name = 10,
        // Symbol = 11,
        // TotalSupply = 12,
        BalanceOf = 13,
        Transfer = 14,
        // TransferFrom = 15,
        // IncreaseAllowance = 16,
        // DecreaseAllowance = 17,
        // RevokeAllowance = 18,
        Burn = 19,
        // BurnFrom = 20,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
    pub struct MintParams {
        pub to: Address,
        pub amount: TokenAmount,
        pub operators: Vec<Address>,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
    pub struct DestroyParams {
        pub owner: Address,
        pub amount: TokenAmount,
    }
}
