// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_types::{RegisteredSealProof, StoragePower};

/// Returns the minimum storage power required for each seal proof types.
pub fn consensus_miner_min_power(p: RegisteredSealProof) -> Result<StoragePower, String> {
    use RegisteredSealProof::*;
    match p {
        // Specs actors defaults to other values, these are the mainnet values put in place
        StackedDRG2KiBV1 | StackedDRG2KiBV1P1 | StackedDRG512MiBV1 | StackedDRG512MiBV1P1
        | StackedDRG8MiBV1 | StackedDRG8MiBV1P1 | StackedDRG32GiBV1 | StackedDRG32GiBV1P1
        | StackedDRG64GiBV1 | StackedDRG64GiBV1P1 => {
            if cfg!(feature = "devnet") {
                return Ok(StoragePower::from(2048));
            }
            if cfg!(feature = "interopnet") {
                return Ok(StoragePower::from(2 << 30));
            }

            Ok(StoragePower::from(10u64 << 40))
        }
        Invalid(i) => Err(format!("unsupported proof type: {}", i)),
    }
}
