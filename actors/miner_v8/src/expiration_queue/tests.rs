// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_actors_runtime_v8::DealWeight;
use fvm_shared::clock::NO_QUANTIZATION;
use fvm_shared::sector::StoragePower;

#[test]
fn test_expirations() {
    let quant = QuantSpec {
        unit: 10,
        offset: 3,
    };
    let sectors = [
        test_sector(7, 1, 0, 0, 0),
        test_sector(8, 2, 0, 0, 0),
        test_sector(14, 3, 0, 0, 0),
        test_sector(13, 4, 0, 0, 0),
    ];
    let result = group_new_sectors_by_declared_expiration(SectorSize::_2KiB, &sectors, quant);
    let expected = [
        SectorEpochSet {
            epoch: 13,
            sectors: vec![1, 2, 4],
            power: PowerPair {
                raw: StoragePower::from(2048 * 3),
                qa: StoragePower::from(2048 * 3),
            },
            pledge: Zero::zero(),
        },
        SectorEpochSet {
            epoch: 23,
            sectors: vec![3],
            power: PowerPair {
                raw: StoragePower::from(2048),
                qa: StoragePower::from(2048),
            },
            pledge: Zero::zero(),
        },
    ];
    assert_eq!(expected.len(), result.len());
    for (i, ex) in expected.iter().enumerate() {
        assert_sector_set(ex, &result[i]);
    }
}

#[test]
fn test_expirations_empty() {
    let sectors = Vec::new();
    let result =
        group_new_sectors_by_declared_expiration(SectorSize::_2KiB, sectors, NO_QUANTIZATION);
    assert!(result.is_empty());
}

fn assert_sector_set(expected: &SectorEpochSet, actual: &SectorEpochSet) {
    assert_eq!(expected.epoch, actual.epoch);
    assert_eq!(expected.sectors, actual.sectors);
    assert_eq!(expected.power, actual.power);
    assert_eq!(expected.pledge, actual.pledge);
}

fn test_sector(
    expiration: ChainEpoch,
    sector_number: SectorNumber,
    deal_weight: u64,
    verified_deal_weight: u64,
    initial_pledge: u64,
) -> SectorOnChainInfo {
    SectorOnChainInfo {
        expiration,
        sector_number,
        deal_weight: DealWeight::from(deal_weight),
        verified_deal_weight: DealWeight::from(verified_deal_weight),
        initial_pledge: TokenAmount::from_atto(initial_pledge),
        ..Default::default()
    }
}
