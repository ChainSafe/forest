// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_types::{RegisteredPoStProof, StoragePower};

/// Returns the minimum storage power required for each seal proof types.
pub fn consensus_miner_min_power(p: RegisteredPoStProof) -> Result<StoragePower, String> {
    use RegisteredPoStProof::*;
    match p {
        StackedDRGWinning2KiBV1
        | StackedDRGWinning8MiBV1
        | StackedDRGWinning512MiBV1
        | StackedDRGWinning32GiBV1
        | StackedDRGWinning64GiBV1
        | StackedDRGWindow2KiBV1
        | StackedDRGWindow8MiBV1
        | StackedDRGWindow512MiBV1
        | StackedDRGWindow32GiBV1
        | StackedDRGWindow64GiBV1 => {
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
