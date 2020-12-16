// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_types::{RegisteredSealProof, StoragePower};

/// Returns the minimum storage power required for each seal proof types.
#[cfg(not(feature = "devnet"))]
pub fn consensus_miner_min_power(p: RegisteredSealProof) -> Result<StoragePower, String> {
    use RegisteredSealProof::*;
    match p {
        StackedDRG2KiBV1 | StackedDRG2KiBV1P1 => Ok(StoragePower::from(0)),
        StackedDRG512MiBV1 | StackedDRG512MiBV1P1 => Ok(StoragePower::from(16 << 20)),
        StackedDRG8MiBV1 | StackedDRG8MiBV1P1 => Ok(StoragePower::from(1 << 30)),
        StackedDRG32GiBV1 | StackedDRG32GiBV1P1 => Ok(StoragePower::from(10u64 << 40)),
        StackedDRG64GiBV1 | StackedDRG64GiBV1P1 => Ok(StoragePower::from(20u64 << 40)),
        Invalid(i) => Err(format!("unsupported proof type: {}", i)),
    }
}

/// Returns the minimum storage power required for each seal proof types.
#[cfg(feature = "devnet")]
pub fn consensus_miner_min_power(_p: RegisteredSealProof) -> Result<StoragePower, String> {
    Ok(StoragePower::from(2048))
}
