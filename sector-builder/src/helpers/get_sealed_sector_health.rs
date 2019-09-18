use crate::helpers;
use crate::{SealedSectorHealth, SealedSectorMetadata};
use std::path::Path;

pub fn get_sealed_sector_health<T: AsRef<Path>>(
    sealed_sector_path: T,
    meta: &SealedSectorMetadata,
) -> Result<SealedSectorHealth, failure::Error> {
    let result = std::fs::metadata(&sealed_sector_path);

    // ensure that the file still exists
    if result.is_err() {
        return Ok(SealedSectorHealth::ErrorMissing);
    }

    // compare lengths
    if result?.len() != meta.len {
        return Ok(SealedSectorHealth::ErrorInvalidLength);
    }

    // compare checksums
    if helpers::checksum::calculate_checksum(&sealed_sector_path)?.as_bytes()
        != meta.blake2b_checksum.as_slice()
    {
        return Ok(SealedSectorHealth::ErrorInvalidChecksum);
    }

    Ok(SealedSectorHealth::Ok)
}
