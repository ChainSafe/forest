// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{errors::SerdeError, BYTE_ARRAY_MAX_LEN};
use serde::{de, ser, Deserializer, Serializer};
use serde_bytes::{Deserialize, Serialize};

/// checked if input > `crate::BYTE_ARRAY_MAX_LEN`
pub fn serialize<T, S>(bytes: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: ?Sized + Serialize + AsRef<[u8]>,
    S: Serializer,
{
    let len = bytes.as_ref().len();
    if len > BYTE_ARRAY_MAX_LEN {
        return Err(ser::Error::custom(SerdeError::ByteArrayExceedsMaxLength(
            len,
        )));
    }

    Serialize::serialize(bytes, serializer)
}

/// checked if output > `crate::ByteArrayMaxLen`
pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + AsRef<[u8]>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).and_then(|bytes: T| {
        let len = bytes.as_ref().len();
        if len > BYTE_ARRAY_MAX_LEN {
            Err(de::Error::custom(SerdeError::ByteArrayExceedsMaxLength(
                len,
            )))
        } else {
            Ok(bytes)
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::{serde_byte_array, SerdeError, BYTE_ARRAY_MAX_LEN};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct ByteArray {
        #[serde(with = "serde_byte_array")]
        pub inner: Vec<u8>,
    }

    #[test]
    fn can_serialize_byte_array() {
        for len in [0, 1, BYTE_ARRAY_MAX_LEN] {
            let bytes = ByteArray {
                inner: vec![0; len],
            };

            assert!(serde_cbor::to_vec(&bytes).is_ok());
        }
    }

    #[test]
    fn cannot_serialize_byte_array_overflow() {
        let len = BYTE_ARRAY_MAX_LEN + 1;
        let bytes = ByteArray {
            inner: vec![0; len],
        };

        assert_eq!(
            serde_cbor::to_vec(&bytes).err().unwrap().to_string(),
            SerdeError::ByteArrayExceedsMaxLength(len).to_string()
        );
    }

    #[test]
    fn can_deserialize_byte_array() {
        for len in [0, 1, BYTE_ARRAY_MAX_LEN] {
            let bytes = ByteArray {
                inner: vec![0; len],
            };

            let encoding = serde_cbor::to_vec(&bytes).unwrap();
            assert_eq!(
                serde_cbor::from_slice::<ByteArray>(&encoding).unwrap(),
                bytes
            );
        }
    }

    #[test]
    fn cannot_deserialize_byte_array_overflow() {
        let max_length_bytes = ByteArray {
            inner: vec![0; BYTE_ARRAY_MAX_LEN],
        };

        // prefix: 2 ^ 21 -> 2 ^ 21 + 1
        let mut overflow_encoding = serde_cbor::to_vec(&max_length_bytes).unwrap();
        let encoding_len = overflow_encoding.len();
        overflow_encoding[encoding_len - BYTE_ARRAY_MAX_LEN - 1] = 1;
        overflow_encoding.push(0);

        assert_eq!(
            serde_cbor::from_slice::<ByteArray>(&overflow_encoding)
                .err()
                .unwrap()
                .to_string(),
            SerdeError::ByteArrayExceedsMaxLength(BYTE_ARRAY_MAX_LEN + 1).to_string()
        );
    }
}
