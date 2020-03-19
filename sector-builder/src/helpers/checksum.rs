use std::convert::AsRef;

/// Calculates the BLAKE2b checksum of a given file.
pub fn calculate_checksum(
    path: impl AsRef<std::path::Path>,
) -> std::io::Result<blake2b_simd::Hash> {
    let mut hasher = blake2b_simd::blake2bp::State::new();
    let f = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(f);
    std::io::copy(&mut reader, &mut hasher)?;
    Ok(hasher.finalize())
}
