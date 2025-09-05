// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use num::Zero;

impl DeadlineExt for Deadline {
    fn daily_fee(&self) -> TokenAmount {
        use Deadline::*;
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
            V17(d) => (&d.daily_fee).into(),
        }
    }
}
