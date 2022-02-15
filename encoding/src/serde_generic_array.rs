// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![macro_use]
use crate::GENERIC_ARRAY_MAX_LEN;
use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};

const EXCEED_MAX_LENGTH: &str = "Array exceed max length";

/// check length for generic array
pub fn check_length<T>(generic_array: &[T]) -> Result<(), &str> {
    if generic_array.len() > GENERIC_ARRAY_MAX_LEN {
        return Err(EXCEED_MAX_LENGTH);
    }

    Ok(())
}

#[macro_export]
macro_rules! check_generic_array_length {
    ($arr:expr) => {
        check_length($arr)
    };
    ($( $arr:expr ),+) => {
        [
            $( check_length($arr) ),+
        ].iter().cloned().collect::<Result<Vec<_>, &str>>();
    };
}

/// trait for getting length of `Vec<T>`
pub trait GenericArray {
    fn is_empty(&self) -> bool;

    fn len(&self) -> usize;
}

impl<T> GenericArray for Vec<T> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn len(&self) -> usize {
        self.len()
    }
}

/// checked if input > `crate::BYTE_ARRAY_MAX_LEN`
pub fn serialize<T, S>(generic_array: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: ?Sized + Serialize + GenericArray,
    S: Serializer,
{
    if generic_array.len() > GENERIC_ARRAY_MAX_LEN {
        return Err(ser::Error::custom::<String>(EXCEED_MAX_LENGTH.into()));
    }

    Serialize::serialize(generic_array, serializer)
}

/// checked if output > `crate::ByteArrayMaxLen`
pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + GenericArray,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).and_then(|generic_array: T| {
        if generic_array.len() > GENERIC_ARRAY_MAX_LEN {
            Err(de::Error::custom::<String>(EXCEED_MAX_LENGTH.into()))
        } else {
            Ok(generic_array)
        }
    })
}
