use ferret_bigint::UBigInt;

/// Wrapper around a big int variable to handle token specific functionality
// TODO verify on finished spec whether or not big int or uint
#[derive(Default, Clone, PartialEq, Debug)]
pub struct TokenAmount(pub UBigInt);

impl TokenAmount {
    pub fn new(val: u64) -> Self {
        TokenAmount(UBigInt::from(val))
    }
}
