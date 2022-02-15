// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![macro_use]
use crate::GENERIC_ARRAY_MAX_LEN;

/// check length for generic array
pub fn check_length<T>(generic_array: &[T]) -> Result<(), &str> {
    if generic_array.len() > GENERIC_ARRAY_MAX_LEN {
        return Err("Array exceed max length");
    }

    Ok(())
}

#[macro_export]
macro_rules! check_generic_array_length {
    ($arr:expr) => {
        check_length($arr)
    };
    ($( $arr:expr ),+) => {
        [
            $( check_length($arr) ),+
        ].iter().cloned().collect::<Result<Vec<_>, &str>>();
    };
}
