// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod balance_table;

use crate::shim::address::Address;
use crate::shim::econ::TokenAmount;

pub trait BalanceTableExt {
    fn for_each<F>(&self, f: F) -> anyhow::Result<()>
    where
        F: FnMut(&Address, &TokenAmount) -> anyhow::Result<()>;
}
