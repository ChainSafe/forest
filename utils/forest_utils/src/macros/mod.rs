// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// Creates a constant value from an expression that returns an Option that we
/// *know* is not None. Basically a workaround till <https://github.com/rust-lang/rust/issues/67441> is stabilized.
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

/// Retries a function call until `max_retries` is exceeded with a delay
#[macro_export]
macro_rules! retry {
    ($func:ident, $max_retries:expr, $delay:expr $(, $arg:expr)*) => {{
        let mut retry_count = 0;
        loop {
            match $func($($arg),*).await {
                Ok(val) => break Ok(val),
                Err(err) => {
                    retry_count += 1;
                    if retry_count >= $max_retries {
                        info!("Maximum retries exceeded.");
                        break Err(err);
                    }
                    info!("Retry attempt {} started with delay {:#?}.", retry_count, $delay);
                    sleep($delay).await;
                }
            }
        }
    }};
}
