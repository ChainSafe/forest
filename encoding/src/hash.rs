use blake2::digest::{Input, VariableOutput};
use blake2::VarBlake2b;

/// generates blake2b hash with provided size
pub fn variable_hash(ingest: Vec<u8>, size: usize) -> Vec<u8> {
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
