// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use core::marker::PhantomData;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

pub use max_len::*;
pub use types::*;

mod types {
    use super::*;

    /// Byte array with BYTE_ARRAY_MAX_LEN as limit length
    pub type ByteArray = BoundedVec<u8, ByteArrayMaxLen>;

    /// Byte array with BYTE_ARRAY_MAX_LEN as limit length
    pub type GenericArray<T> = BoundedVec<T, GenericArrayMaxLen>;
}

mod max_len {
    /// Trait for defining length limit for `BoundedVec`
    pub trait MaxLen {
        fn max_len() -> usize;
    }

    /// Instance of `BYTE_ARRAY_MAX_LEN`
    pub struct ByteArrayMaxLen;

    impl MaxLen for ByteArrayMaxLen {
        fn max_len() -> usize {
            crate::BYTE_ARRAY_MAX_LEN
        }
    }

    /// Instance of `GENERIC_ARRAY_MAX_LEN`
    pub struct GenericArrayMaxLen;

    impl MaxLen for GenericArrayMaxLen {
        fn max_len() -> usize {
            crate::GENERIC_ARRAY_MAX_LEN
        }
    }
}

/// A bounded vector.
pub struct BoundedVec<T, L>(pub Vec<T>, PhantomData<L>);

impl<T, L: MaxLen> BoundedVec<T, L> {
    pub fn new(inner: Vec<T>) -> Self {
        Self(inner, Default::default())
    }
}

impl<T: Serialize, L: MaxLen> Serialize for BoundedVec<T, L>
where
    [T]: serde_bytes::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        <[T] as serde_bytes::Serialize>::serialize(&self.0, serializer)
    }
}

impl<'de, T: Deserialize<'de>, L: MaxLen> Deserialize<'de> for BoundedVec<T, L>
where
    [T]: serde_bytes::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let output: Vec<T> = Deserialize::deserialize(deserializer)?;
        if output.len() >= L::max_len() {
            return Err(de::Error::custom(format!(
                "Array exceed max length {}",
                L::max_len()
            )));
        }
        Ok(Self::new(output))
    }
}
