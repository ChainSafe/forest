// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blake2b_simd::Params;

/// Generates BLAKE2b hash of fixed 32 bytes size.
///
/// # Example
/// ```
/// use forest_encoding::blake2b_256;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_hashing() {
        let ing_vec = vec![1, 2, 3];

        assert_eq!(blake2b_256(&ing_vec), blake2b_256(&[1, 2, 3]));
        assert_ne!(blake2b_256(&ing_vec), blake2b_256(&[1, 2, 3, 4]));
    }
}
