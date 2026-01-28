// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub use super::fvm_latest::gas::{
    Gas as Gas_latest, GasCharge as GasCharge_latest, GasDuration as GasDuration_latest,
    GasOutputs as GasOutputs_latest,
};
use crate::shim::econ::TokenAmount;
use crate::shim::version::NetworkVersion;
use fvm2::gas::{
    Gas as GasV2, GasCharge as GasChargeV2, PriceList as PriceListV2,
    price_list_by_network_version as price_list_by_network_version_v2,
};
use fvm3::gas::{
    Gas as GasV3, MILLIGAS_PRECISION,
    price_list_by_network_version as price_list_by_network_version_v3,
};
pub use fvm3::gas::{GasCharge as GasChargeV3, GasTracker, PriceList as PriceListV3};
use fvm4::gas::price_list_by_network_version as price_list_by_network_version_v4;
pub use fvm4::gas::{
    Gas as GasV4, GasCharge as GasChargeV4, GasOutputs as GasOutputsV4, PriceList as PriceListV4,
};
use spire_enum::prelude::delegated_enum;
use std::fmt::{Debug, Display};

#[derive(Hash, Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Default, derive_more::From)]
pub struct Gas(Gas_latest);

#[derive(Clone, Default, derive_more::From, derive_more::Into)]
pub struct GasDuration(GasDuration_latest);

#[derive(Clone, Default, derive_more::From, derive_more::Into)]
pub struct GasOutputs(GasOutputs_latest);

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
        Self(Gas_latest::new(gas))
    }

    pub fn round_up(&self) -> u64 {
        self.0.round_up()
    }
}

impl GasDuration {
    pub fn as_nanos(&self) -> u64 {
        if let Some(duration) = self.0.get() {
            duration.as_nanos().clamp(0, u64::MAX as u128) as u64
        } else {
            0
        }
    }
}

impl From<GasV2> for Gas {
    fn from(value: GasV2) -> Self {
        Gas(Gas_latest::from_milligas(value.as_milligas() as _))
    }
}

impl From<Gas> for GasV2 {
    fn from(value: Gas) -> Self {
        GasV2::from_milligas(value.0.as_milligas() as _)
    }
}

impl From<Gas> for GasV3 {
    fn from(value: Gas) -> Self {
        GasV3::from_milligas(value.0.as_milligas())
    }
}

impl From<GasV3> for Gas {
    fn from(value: GasV3) -> Self {
        Gas(Gas_latest::from_milligas(value.as_milligas() as _))
    }
}

impl From<Gas> for GasV4 {
    fn from(value: Gas) -> Self {
        GasV4::from_milligas(value.0.as_milligas())
    }
}

#[derive(Debug, Clone, derive_more::From, derive_more::Into)]
pub struct GasCharge(GasCharge_latest);

impl GasCharge {
    /// Calculates total gas charge (in `milligas`) by summing compute and
    /// storage gas associated with this charge.
    pub fn total(&self) -> Gas {
        self.0.total().into()
    }
    pub fn name(&self) -> &str {
        &self.0.name
    }
    pub fn compute_gas(&self) -> Gas {
        self.0.compute_gas.into()
    }
    pub fn other_gas(&self) -> Gas {
        self.0.other_gas.into()
    }
    pub fn elapsed(&self) -> GasDuration {
        self.0.elapsed.clone().into()
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
        GasChargeV4 {
            name: value.name,
            compute_gas: GasV4::from_milligas(value.compute_gas.as_milligas()),
            other_gas: GasV4::from_milligas(value.other_gas.as_milligas()),
            elapsed: value.elapsed.get().map(|&d| d.into()).unwrap_or_default(),
        }
        .into()
    }
}

impl From<GasCharge> for GasChargeV2 {
    fn from(value: GasCharge) -> Self {
        Self {
            name: value.0.name,
            compute_gas: GasV2::from_milligas(value.0.compute_gas.as_milligas() as _),
            storage_gas: GasV2::from_milligas(value.0.other_gas.as_milligas() as _),
        }
    }
}

impl From<GasCharge> for GasChargeV3 {
    fn from(value: GasCharge) -> Self {
        Self {
            name: value.0.name,
            compute_gas: GasV3::from_milligas(value.0.compute_gas.as_milligas() as _),
            other_gas: GasV3::from_milligas(value.0.other_gas.as_milligas() as _),
            elapsed: Default::default(),
        }
    }
}

impl GasOutputs {
    pub fn compute(
        // In whole gas units.
        gas_used: u64,
        gas_limit: u64,
        base_fee: &TokenAmount,
        fee_cap: &TokenAmount,
        gas_premium: &TokenAmount,
    ) -> Self {
        GasOutputsV4::compute(gas_used, gas_limit, base_fee, fee_cap, gas_premium).into()
    }

    pub fn total_spent(self) -> TokenAmount {
        (self.0.base_fee_burn + self.0.miner_tip + self.0.over_estimation_burn).into()
    }
}

#[delegated_enum(impl_conversions)]
pub enum PriceList {
    V2(&'static PriceListV2),
    V3(&'static PriceListV3),
    V4(&'static PriceListV4),
}

impl PriceList {
    pub fn on_block_open_base(&self) -> GasCharge {
        delegate_price_list!(self.on_block_open_base().into())
    }

    pub fn on_block_link(&self, data_size: usize) -> GasCharge {
        match self {
            PriceList::V2(list) => list.on_block_link(data_size).into(),
            PriceList::V3(list) => list
                .on_block_link(fvm3::kernel::SupportedHashes::Blake2b256, data_size)
                .into(),
            PriceList::V4(list) => list
                .on_block_link(fvm4::kernel::SupportedHashes::Blake2b256, data_size)
                .into(),
        }
    }

    pub fn on_chain_message(&self, msg_size: usize) -> GasCharge {
        delegate_price_list!(self.on_chain_message(msg_size).into())
    }
}

pub fn price_list_by_network_version(network_version: NetworkVersion) -> PriceList {
    if network_version < NetworkVersion::V18 {
        price_list_by_network_version_v2(network_version.into()).into()
    } else if network_version < NetworkVersion::V21 {
        price_list_by_network_version_v3(network_version.into()).into()
    } else {
        price_list_by_network_version_v4(network_version.into()).into()
    }
}
