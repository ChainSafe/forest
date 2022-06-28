// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod gas_charge;

pub use self::gas_charge::GasCharge;
use fvm::gas::{price_list_by_network_version, PriceList};
use fvm_shared::{clock::ChainEpoch, version::NetworkVersion};

/// Returns gas price list by Epoch for gas consumption.
pub fn price_list_by_epoch(epoch: ChainEpoch, calico_height: ChainEpoch) -> &'static PriceList {
    let version = if epoch < calico_height {
        NetworkVersion::V15
    } else {
        NetworkVersion::V0
    };
    price_list_by_network_version(version)
}
