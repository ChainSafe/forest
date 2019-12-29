pub use num_bigint::BigInt as BaseBigInt;
use std::fmt;
use std::ops::{Deref, DerefMut};

/// Signed Big integer variable
#[derive(PartialEq, Eq, Clone, Debug, Hash, Default, Ord, PartialOrd)]
pub struct BigInt {
    num: BaseBigInt,
}

impl From<i64> for BigInt {
    #[inline]
    fn from(n: i64) -> Self {
        BigInt::from(BaseBigInt::from(n))
    }
}

impl From<BaseBigInt> for BigInt {
    fn from(num: BaseBigInt) -> Self {
        Self { num }
    }
}

impl Deref for BigInt {
    type Target = BaseBigInt;
    fn deref(&self) -> &Self::Target {
        &self.num
    }
}

impl DerefMut for BigInt {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.num
    }
}

impl fmt::Display for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl fmt::Binary for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl fmt::Octal for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl fmt::LowerHex for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl fmt::UpperHex for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.deref().fmt(f)
    }
}
