// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::fmt::{Debug, Display};

pub use super::fvm_latest::gas::{
    Gas as Gas_latest, GasCharge as GasCharge_latest, GasDuration as GasDuration_latest,
};
use fvm2::gas::{
    price_list_by_network_version as price_list_by_network_version_v2, Gas as GasV2,
    GasCharge as GasChargeV2, PriceList as PriceListV2,
};
use fvm3::gas::{
    price_list_by_network_version as price_list_by_network_version_v3, Gas as GasV3,
    MILLIGAS_PRECISION,
};
pub use fvm3::gas::{GasCharge as GasChargeV3, GasTracker, PriceList as PriceListV3};
use fvm4::gas::price_list_by_network_version as price_list_by_network_version_v4;
pub use fvm4::gas::{
    Gas as GasV4, GasCharge as GasChargeV4, GasDuration as GasDurationV4, PriceList as PriceListV4,
};

use crate::shim::version::NetworkVersion;

#[derive(Hash, Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Default)]
pub struct Gas(Gas_latest);

#[derive(Clone, Default)]
pub struct GasDuration(GasDuration_latest);

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

impl From<GasV4> for Gas {
    fn from(value: GasV4) -> Self {
        Gas(value)
    }
}

#[derive(Debug, Clone)]
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

impl From<GasChargeV4> for GasCharge {
    fn from(value: GasChargeV4) -> Self {
        GasCharge(value)
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
            // TODO(hanabi1224): https://github.com/ChainSafe/forest/issues/3524
            elapsed: Default::default(),
        }
    }
}

impl From<GasCharge> for GasChargeV4 {
    fn from(value: GasCharge) -> Self {
        value.0
    }
}

impl From<GasDurationV4> for GasDuration {
    fn from(value: GasDurationV4) -> Self {
        GasDuration(value)
    }
}

pub enum PriceList {
    V2(&'static PriceListV2),
    V3(&'static PriceListV3),
    V4(&'static PriceListV4),
}

impl PriceList {
    pub fn on_block_open_base(&self) -> GasCharge {
        match self {
            PriceList::V2(list) => list.on_block_open_base().into(),
            PriceList::V3(list) => list.on_block_open_base().into(),
            PriceList::V4(list) => list.on_block_open_base().into(),
        }
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
        match self {
            PriceList::V2(list) => list.on_chain_message(msg_size).into(),
            PriceList::V3(list) => list.on_chain_message(msg_size).into(),
            PriceList::V4(list) => list.on_chain_message(msg_size).into(),
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

impl From<&'static PriceListV4> for PriceList {
    fn from(value: &'static PriceListV4) -> Self {
        PriceList::V4(value)
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

// TODO(elmattic): https://github.com/ChainSafe/forest/issues/4759
use crate::shim::econ::TokenAmount;

#[derive(Clone, Default)]
pub(crate) struct GasOutputs {
    pub base_fee_burn: TokenAmount,
    pub over_estimation_burn: TokenAmount,
    pub miner_penalty: TokenAmount,
    pub miner_tip: TokenAmount,
    pub refund: TokenAmount,

    // In whole gas units.
    pub gas_refund: u64,
    pub gas_burned: u64,
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
        let mut base_fee_to_pay = base_fee;

        let mut out = GasOutputs::default();

        if base_fee > fee_cap {
            base_fee_to_pay = fee_cap;
            out.miner_penalty = (base_fee - fee_cap.clone()) * gas_used
        }

        out.base_fee_burn = base_fee_to_pay * gas_used;

        let mut miner_tip = gas_premium.clone();
        if &(base_fee_to_pay + &miner_tip) > fee_cap {
            miner_tip = fee_cap - base_fee_to_pay.clone();
        }
        out.miner_tip = &miner_tip * gas_limit;

        let (out_gas_refund, out_gas_burned) = compute_gas_overestimation_burn(gas_used, gas_limit);
        out.gas_refund = out_gas_refund;
        out.gas_burned = out_gas_burned;

        if out.gas_burned != 0 {
            out.over_estimation_burn = base_fee_to_pay * out.gas_burned;
            out.miner_penalty += (base_fee - base_fee_to_pay.clone()) * out.gas_burned;
        }
        let required_funds = fee_cap * gas_limit;
        let refund =
            required_funds - &out.base_fee_burn - &out.miner_tip - &out.over_estimation_burn;
        out.refund = refund;

        out
    }
}

fn compute_gas_overestimation_burn(gas_used: u64, gas_limit: u64) -> (u64, u64) {
    const GAS_OVERUSE_NUM: u128 = 11;
    const GAS_OVERUSE_DENOM: u128 = 10;

    if gas_used == 0 {
        return (0, gas_limit);
    }

    // Convert to u128 to prevent overflow on multiply.
    let gas_used = gas_used as u128;
    let gas_limit = gas_limit as u128;

    // This burns (N-10)% (clamped at 0% and 100%) of the remaining gas where N is the
    // overestimation percentage.
    let over = gas_limit
        .saturating_sub((GAS_OVERUSE_NUM * gas_used) / GAS_OVERUSE_DENOM)
        .min(gas_used);

    // We handle the case where the gas used exceeds the gas limit, just in case.
    let gas_remaining = gas_limit.saturating_sub(gas_used);

    // This computes the fraction of the "remaining" gas to burn and will never be greater than 100%
    // of the remaining gas.
    let gas_to_burn = (gas_remaining * over) / gas_used;

    // But... we use saturating sub, just in case.
    let refund = gas_remaining.saturating_sub(gas_to_burn);

    (refund as u64, gas_to_burn as u64)
}
