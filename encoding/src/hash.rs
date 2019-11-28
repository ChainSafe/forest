use blake2::digest::{Input, VariableOutput};
use blake2::VarBlake2b;

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
    let mut hasher = VarBlake2b::new(size).unwrap();
    hasher.input(ingest);

    // allocate hash result vector
    let mut result: Vec<u8> = vec![0; size];

    hasher.variable_result(|res| {
        // Copy result slice to vector return
        result[..size].clone_from_slice(res);
    });

    result
}

/// generates blake2b hash of fixed 32 bytes size
///
/// # Example
/// ```
/// use encoding::blake2b_256;
///
/// let ingest: Vec<u8> = vec![];
///
/// let mut hash = [0u8; 32];
/// blake2b_256(ingest, &mut hash);
/// ```
pub fn blake2b_256(ingest: Vec<u8>, hash: &mut [u8; 32]) {
    let mut hasher = VarBlake2b::new(32).unwrap();
    hasher.input(ingest);

    hasher.variable_result(|res| {
        // Copy result slice to hash array reference
        hash[..32].clone_from_slice(res);
    });
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
