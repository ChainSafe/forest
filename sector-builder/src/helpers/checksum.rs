use blake2b_simd::Hash;
use std::path::Path;

use crate::error::Result;

/// Calculates the BLAKE2b checksum of a given file.
pub async fn calculate_checksum<T: AsRef<Path>>(path: T) -> Result<Hash> {
    let path = path.as_ref().to_path_buf();
    async_std::task::spawn_blocking(move || {
        let mut hasher = blake2b_simd::blake2bp::State::new();
        let f = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(f);
        std::io::copy(&mut reader, &mut hasher)?;

        Ok(hasher.finalize())
    })
    .await
}
