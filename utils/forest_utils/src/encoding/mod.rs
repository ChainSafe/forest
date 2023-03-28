// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blake2b_simd::Params;
use filecoin_proofs_api::ProverId;
use forest_shim::address::Address;
use fvm_ipld_encoding3::strict_bytes::{Deserialize, Serialize};
pub use serde::{de, ser, Deserializer, Serializer};

/// `serde_bytes` with max length check
pub mod serde_byte_array {
    use super::*;
    /// lotus use cbor-gen for generating codec for types, it has a length limit
    /// for byte array as `2 << 20`
    ///
    /// <https://github.com/whyrusleeping/cbor-gen/blob/f57984553008dd4285df16d4ec2760f97977d713/gen.go#L16>
    pub const BYTE_ARRAY_MAX_LEN: usize = 2 << 20;

    /// checked if `input > crate::BYTE_ARRAY_MAX_LEN`
    pub fn serialize<T, S>(bytes: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: ?Sized + Serialize + AsRef<[u8]>,
        S: Serializer,
    {
        let len = bytes.as_ref().len();
        if len > BYTE_ARRAY_MAX_LEN {
            return Err(ser::Error::custom::<String>(
                "Array exceed max length".into(),
            ));
        }

        Serialize::serialize(bytes, serializer)
    }

    /// checked if `output > crate::ByteArrayMaxLen`
    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: Deserialize<'de> + AsRef<[u8]>,
        D: Deserializer<'de>,
    {
        Deserialize::deserialize(deserializer).and_then(|bytes: T| {
            if bytes.as_ref().len() > BYTE_ARRAY_MAX_LEN {
                Err(de::Error::custom::<String>(
                    "Array exceed max length".into(),
                ))
            } else {
                Ok(bytes)
            }
        })
    }
}

/// Generates BLAKE2b hash of fixed 32 bytes size.
///
/// # Example
/// ```
/// use forest_utils::encoding::blake2b_256;
///
/// let ingest: Vec<u8> = vec![];
/// let hash = blake2b_256(&ingest);
/// assert_eq!(hash.len(), 32);
/// ```
pub fn blake2b_256(ingest: &[u8]) -> [u8; 32] {
    let digest = Params::new()
        .hash_length(32)
        .to_state()
        .update(ingest)
        .finalize();

    let mut ret = [0u8; 32];
    ret.clone_from_slice(digest.as_bytes());
    ret
}

pub fn prover_id_from_u64(id: u64) -> ProverId {
    let mut prover_id = ProverId::default();
    let prover_bytes = Address::new_id(id).payload().to_raw_bytes();
    prover_id[..prover_bytes.len()].copy_from_slice(&prover_bytes);
    prover_id
}

#[cfg(test)]
mod tests {
    use anyhow::{ensure, Result};
    use rand::Rng;
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::encoding::serde_byte_array::BYTE_ARRAY_MAX_LEN;

    #[test]
    fn vector_hashing() {
        let ing_vec = vec![1, 2, 3];

        assert_eq!(blake2b_256(&ing_vec), blake2b_256(&[1, 2, 3]));
        assert_ne!(blake2b_256(&ing_vec), blake2b_256(&[1, 2, 3, 4]));
    }

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

            assert!(serde_ipld_dagcbor::to_vec(&bytes).is_ok());
        }
    }

    #[test]
    fn cannot_serialize_byte_array_overflow() -> Result<()> {
        let bytes = ByteArray {
            inner: vec![0; BYTE_ARRAY_MAX_LEN + 1],
        };

        ensure!(
            format!("{}", serde_ipld_dagcbor::to_vec(&bytes).err().unwrap())
                .contains("Array exceed max length")
        );

        Ok(())
    }

    #[test]
    fn can_deserialize_byte_array() {
        for len in [0, 1, BYTE_ARRAY_MAX_LEN] {
            let bytes = ByteArray {
                inner: vec![0; len],
            };

            let encoding = serde_ipld_dagcbor::to_vec(&bytes).unwrap();
            assert_eq!(
                serde_ipld_dagcbor::from_slice::<ByteArray>(&encoding).unwrap(),
                bytes
            );
        }
    }

    #[test]
    fn cannot_deserialize_byte_array_overflow() -> Result<()> {
        let max_length_bytes = ByteArray {
            inner: vec![0; BYTE_ARRAY_MAX_LEN],
        };

        // prefix: 2 ^ 21 -> 2 ^ 21 + 1
        let mut overflow_encoding = serde_ipld_dagcbor::to_vec(&max_length_bytes).unwrap();
        let encoding_len = overflow_encoding.len();
        overflow_encoding[encoding_len - BYTE_ARRAY_MAX_LEN - 1] = 1;
        overflow_encoding.push(0);

        ensure!(format!(
            "{}",
            serde_ipld_dagcbor::from_slice::<ByteArray>(&overflow_encoding)
                .err()
                .unwrap()
        )
        .contains("Array exceed max length"));
        Ok(())
    }

    #[test]
    fn parity_tests() -> anyhow::Result<()> {
        use cs_serde_bytes;

        #[derive(Deserialize, Serialize)]
        struct A(#[serde(with = "fvm_ipld_encoding3::strict_bytes")] Vec<u8>);

        #[derive(Deserialize, Serialize)]
        struct B(#[serde(with = "cs_serde_bytes")] Vec<u8>);

        let mut array = [0; 1024];
        rand::rngs::OsRng.fill(&mut array);

        let a = A(array.to_vec());
        let b = B(array.to_vec());

        ensure!(serde_json::to_string_pretty(&a)? == serde_json::to_string_pretty(&b)?);

        Ok(())
    }
}
