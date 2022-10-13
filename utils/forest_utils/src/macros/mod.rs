// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// Creates a constant value from an expression that returns an Option that we *know* is not None.
/// Basically a workaround till <https://github.com/rust-lang/rust/issues/67441> is stabilized.
///
/// # Example
/// ```
/// use forest_utils::const_option;
/// const MY_CONST: i32 = const_option!(Some(42));
/// ```
///
/// # This will at fail compile-time.
/// ```compile_fail
/// use forest_utils::const_option;
/// const MY_CONST: i32 = const_option!(None);
/// ```
#[macro_export]
macro_rules! const_option {
    ($value:expr) => {
        match $value {
            Some(v) => v,
            None => {
                const error_msg: &str = $crate::const_format::concatcp!(
                    "Failed on unwrapping expression ",
                    stringify!($value)
                );
                panic!("{}", error_msg);
            }
        }
    };
}
