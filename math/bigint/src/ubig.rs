pub use num_bigint::BigUint as BaseUBigInt;
use std::fmt;
use std::ops::{Deref, DerefMut};

#[derive(PartialEq, Eq, Clone, Debug, Hash, Default)]
pub struct UBigInt {
    num: BaseUBigInt,
}

impl From<u64> for UBigInt {
    #[inline]
    fn from(n: u64) -> Self {
        UBigInt::from(BaseUBigInt::from(n))
    }
}

impl From<BaseUBigInt> for UBigInt {
    fn from(num: BaseUBigInt) -> Self {
        Self { num }
    }
}

impl Deref for UBigInt {
    type Target = BaseUBigInt;
    fn deref(&self) -> &Self::Target {
        &self.num
    }
}

impl DerefMut for UBigInt {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.num
    }
}

impl fmt::Display for UBigInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl fmt::Binary for UBigInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl fmt::Octal for UBigInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl fmt::LowerHex for UBigInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl fmt::UpperHex for UBigInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.deref().fmt(f)
    }
}
