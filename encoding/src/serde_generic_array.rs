// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![macro_use]
use crate::{SerdeError, GENERIC_ARRAY_MAX_LEN};
use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};

/// check length for generic array
pub fn check_length<T>(generic_array: &[T]) -> Result<(), SerdeError> {
    let len = generic_array.len();
    if len > GENERIC_ARRAY_MAX_LEN {
        return Err(SerdeError::GenericArrayExceedsMaxLength(len));
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
        ].into_iter().collect::<Result<Vec<_>, SerdeError>>();
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
    let len = generic_array.len();
    if len > GENERIC_ARRAY_MAX_LEN {
        return Err(ser::Error::custom(
            SerdeError::GenericArrayExceedsMaxLength(len),
        ));
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
        let len = generic_array.len();
        if len > GENERIC_ARRAY_MAX_LEN {
            Err(de::Error::custom(SerdeError::GenericArrayExceedsMaxLength(
                len,
            )))
        } else {
            Ok(generic_array)
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::{serde_generic_array, SerdeError, GENERIC_ARRAY_MAX_LEN};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct GenericArray {
        #[serde(with = "serde_generic_array")]
        pub inner: Vec<u8>,
    }

    #[test]
    fn can_serialize_generic_array() {
        for len in [0, 1, GENERIC_ARRAY_MAX_LEN] {
            let arr = GenericArray {
                inner: vec![0; len],
            };

            assert!(serde_cbor::to_vec(&arr).is_ok());
        }
    }

    #[test]
    fn cannot_serialize_generic_array_overflow() {
        let len = GENERIC_ARRAY_MAX_LEN + 1;
        let arr = GenericArray {
            inner: vec![0; len],
        };

        assert_eq!(
            serde_cbor::to_vec(&arr).err().unwrap().to_string(),
            SerdeError::GenericArrayExceedsMaxLength(len).to_string()
        );
    }

    #[test]
    fn can_deserialize_generic_array() {
        for len in [0, 1, GENERIC_ARRAY_MAX_LEN] {
            let arr = GenericArray {
                inner: vec![0; len],
            };

            let encoding = serde_cbor::to_vec(&arr).unwrap();
            assert_eq!(
                serde_cbor::from_slice::<GenericArray>(&encoding).unwrap(),
                arr
            );
        }
    }

    #[test]
    fn cannot_deserialize_generic_array_overflow() {
        let max_length_generic_array = GenericArray {
            inner: vec![0; GENERIC_ARRAY_MAX_LEN],
        };

        // prefix: 2 ^ 21 -> 2 ^ 21 + 1
        let mut overflow_encoding = serde_cbor::to_vec(&max_length_generic_array).unwrap();
        let encoding_len = overflow_encoding.len();
        overflow_encoding[encoding_len - GENERIC_ARRAY_MAX_LEN - 1] = 1;
        overflow_encoding.push(0);

        assert_eq!(
            serde_cbor::from_slice::<GenericArray>(&overflow_encoding)
                .err()
                .unwrap()
                .to_string(),
            SerdeError::GenericArrayExceedsMaxLength(GENERIC_ARRAY_MAX_LEN + 1).to_string()
        );
    }
}
