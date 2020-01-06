// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use blake2b_simd::Params;

/// generates blake2b hash with provided size
///
/// # Example
/// ```
/// use encoding::blake2b_variable;
///
/// let ingest: Vec<u8> = vec![];
/// let hash = blake2b_variable(ingest, 20);
/// assert_eq!(hash.len(), 20);
/// ```
pub fn blake2b_variable(ingest: Vec<u8>, size: usize) -> Vec<u8> {
    let hash = Params::new()
        .hash_length(size)
        .to_state()
        .update(&ingest)
        .finalize();

    hash.as_bytes().to_vec()
}

/// generates blake2b hash of fixed 32 bytes size
///
/// # Example
/// ```
/// use encoding::blake2b_256;
///
/// let ingest: Vec<u8> = vec![];
/// let hash = blake2b_256(ingest);
/// assert_eq!(hash.len(), 32);
/// ```
pub fn blake2b_256(ingest: Vec<u8>) -> [u8; 32] {
    let digest = Params::new()
        .hash_length(32)
        .to_state()
        .update(&ingest)
        .finalize();

    let mut ret = [0u8; 32];
    ret.clone_from_slice(digest.as_bytes());
    ret
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_length() {
        let ingest = vec![1, 4, 2, 3];
        let hash = blake2b_variable(ingest.clone(), 8);
        assert_eq!(hash.len(), 8);
        let hash = blake2b_variable(ingest.clone(), 20);
        assert_eq!(hash.len(), 20);
        let hash = blake2b_variable(ingest.clone(), 32);
        assert_eq!(hash.len(), 32);
    }
}
