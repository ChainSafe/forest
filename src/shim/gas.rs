// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::fmt::{Debug, Display};

use fvm::gas::{
    price_list_by_network_version as price_list_by_network_version_v2, Gas as GasV2,
    GasCharge as GasChargeV2, PriceList as PriceListV2,
};
pub use fvm3::gas::GasTracker;
use fvm3::gas::{
    price_list_by_network_version as price_list_by_network_version_v3, Gas as GasV3,
    GasCharge as GasChargeV3, PriceList as PriceListV3, MILLIGAS_PRECISION,
};

use crate::shim::version::NetworkVersion;

#[derive(Hash, Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Default)]
pub struct Gas(GasV3);

impl Debug for Gas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.as_milligas() == 0 {
            f.debug_tuple("Gas").field(&0 as &dyn Debug).finish()
        } else {
            let integral = self.0.as_milligas() / MILLIGAS_PRECISION;
            let fractional = self.0.as_milligas() % MILLIGAS_PRECISION;
            f.debug_tuple("Gas")
                .field(&format_args!("{integral}.{fractional:03}"))
                .finish()
        }
    }
}

impl Display for Gas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.as_milligas() == 0 {
            f.write_str("0")
        } else {
            let integral = self.0.as_milligas() / MILLIGAS_PRECISION;
            let fractional = self.0.as_milligas() % MILLIGAS_PRECISION;
            write!(f, "{integral}.{fractional:03}")
        }
    }
}

impl Gas {
    pub fn new(gas: u64) -> Self {
        Self(GasV3::new(gas))
    }

    pub fn round_up(&self) -> u64 {
        self.0.round_up()
    }
}

impl From<GasV2> for Gas {
    fn from(value: GasV2) -> Self {
        Gas(GasV3::from_milligas(value.as_milligas() as u64))
    }
}

impl From<Gas> for GasV2 {
    fn from(value: Gas) -> Self {
        GasV2::from_milligas(value.0.as_milligas() as i64)
    }
}

impl From<Gas> for GasV3 {
    fn from(value: Gas) -> Self {
        GasV3::from_milligas(value.0.as_milligas())
    }
}

impl From<GasV3> for Gas {
    fn from(value: GasV3) -> Self {
        Gas(value)
    }
}

pub struct GasCharge(GasChargeV3);

impl GasCharge {
    /// Calculates total gas charge (in `milligas`) by summing compute and
    /// storage gas associated with this charge.
    pub fn total(&self) -> Gas {
        self.0.total().into()
    }
}

impl From<GasChargeV2> for GasCharge {
    fn from(value: GasChargeV2) -> Self {
        GasChargeV3 {
            name: value.name,
            compute_gas: GasV3::from_milligas(value.compute_gas.as_milligas() as u64),
            other_gas: GasV3::from_milligas(value.storage_gas.as_milligas() as u64),
            elapsed: Default::default(),
        }
        .into()
    }
}

impl From<GasChargeV3> for GasCharge {
    fn from(value: GasChargeV3) -> Self {
        GasCharge(value)
    }
}

impl From<GasCharge> for GasChargeV3 {
    fn from(value: GasCharge) -> Self {
        value.0
    }
}

impl From<GasCharge> for GasChargeV2 {
    fn from(value: GasCharge) -> Self {
        GasChargeV2 {
            name: value.0.name,
            compute_gas: GasV2::from_milligas(value.0.compute_gas.as_milligas() as i64),
            storage_gas: GasV2::from_milligas(value.0.other_gas.as_milligas() as i64),
        }
    }
}

pub enum PriceList {
    V2(&'static PriceListV2),
    V3(&'static PriceListV3),
}

impl PriceList {
    pub fn on_block_open_base(&self) -> GasCharge {
        match self {
            PriceList::V2(list) => list.on_block_open_base().into(),
            PriceList::V3(list) => list.on_block_open_base().into(),
        }
    }

    pub fn on_block_link(&self, data_size: usize) -> GasCharge {
        match self {
            PriceList::V2(list) => list.on_block_link(data_size).into(),
            PriceList::V3(list) => list
                .on_block_link(fvm3::kernel::SupportedHashes::Blake2b256, data_size)
                .into(),
        }
    }

    pub fn on_chain_message(&self, msg_size: usize) -> GasCharge {
        match self {
            PriceList::V2(list) => list.on_chain_message(msg_size).into(),
            PriceList::V3(list) => list.on_chain_message(msg_size).into(),
        }
    }
}

impl From<&'static PriceListV2> for PriceList {
    fn from(value: &'static PriceListV2) -> Self {
        PriceList::V2(value)
    }
}
impl From<&'static PriceListV3> for PriceList {
    fn from(value: &'static PriceListV3) -> Self {
        PriceList::V3(value)
    }
}

pub fn price_list_by_network_version(network_version: NetworkVersion) -> PriceList {
    if network_version < NetworkVersion::V18 {
        price_list_by_network_version_v2(network_version.into()).into()
    } else {
        price_list_by_network_version_v3(network_version.into()).into()
    }
}
