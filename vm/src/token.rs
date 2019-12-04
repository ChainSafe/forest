use num_bigint::BigUint;

/// Wrapper around a big int variable to handle token specific functionality
// TODO verify on finished spec whether or not big int or uint
pub struct TokenAmount(BigUint);
