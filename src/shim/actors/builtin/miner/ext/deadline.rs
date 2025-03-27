// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use num::Zero;
use Deadline::*;

impl DeadlineExt for Deadline {
    fn daily_fee(&self) -> TokenAmount {
        match self {
            V8(_) => Zero::zero(),
            V9(_) => Zero::zero(),
            V10(_) => Zero::zero(),
            V11(_) => Zero::zero(),
            V12(_) => Zero::zero(),
            V13(_) => Zero::zero(),
            V14(_) => Zero::zero(),
            V15(_) => Zero::zero(),
            V16(d) => (&d.daily_fee).into(),
        }
    }

    fn live_power_qa(&self) -> BigInt {
        match self {
            V8(d) => Zero::zero(),
            V9(d) => Zero::zero(),
            V10(d) => Zero::zero(),
            V11(d) => Zero::zero(),
            V12(d) => Zero::zero(),
            V13(d) => Zero::zero(),
            V14(d) => Zero::zero(),
            V15(d) => Zero::zero(),
            V16(d) => d.live_power.qa.clone(),
        }
    }
}
