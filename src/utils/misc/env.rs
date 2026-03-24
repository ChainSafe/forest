// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

/// Get the value of an environment variable, or a default value if it is not set or cannot be
/// parsed.
pub fn env_or_default<T: FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Check if the given environment variable is set to truthy value.
/// Returns false if not set.
pub fn is_env_truthy(env: &str) -> bool {
    is_env_set_and_truthy(env).unwrap_or_default()
}

/// Check if the given environment variable is set to truthy value.
/// Returns None if not set.
pub fn is_env_set_and_truthy(env: &str) -> Option<bool> {
    std::env::var(env)
        .ok()
        .map(|var| matches!(var.to_lowercase().as_str(), "1" | "true" | "yes" | "_yes_"))
}

#[macro_export]
macro_rules! def_is_env_truthy {
    ($fn_name:ident, $env: expr) => {
        #[inline]
        pub fn $fn_name() -> bool {
            cfg_if::cfg_if! {
                if #[cfg(test)] {
                    $crate::utils::misc::env::is_env_truthy($env)
                } else{
                    static ENV: std::sync::LazyLock<bool> =
                        std::sync::LazyLock::new(|| $crate::utils::misc::env::is_env_truthy($env));
                    *ENV
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_or_default() {
        unsafe {
            // variable set, should return its parsed value
            std::env::set_var("TEST_ENV", "42");
            assert_eq!(env_or_default("TEST_ENV", 0), 42);

            // variable not set, should return default
            std::env::remove_var("TEST_ENV");
            assert_eq!(env_or_default("TEST_ENV", 0), 0);

            // unparsable value given the default type, should return default
            std::env::set_var("TEST_ENV", "42");
            assert!(!env_or_default("TEST_ENV", false));
        }
    }

    #[test]
    fn test_is_env_truthy() {
        let cases = [
            ("1", true),
            ("true", true),
            ("0", false),
            ("false", false),
            ("", false),
            ("cthulhu", false),
        ];

        for (input, expected) in cases.iter() {
            unsafe { std::env::set_var("TEST_ENV", input) };
            assert_eq!(is_env_truthy("TEST_ENV"), *expected);
        }
    }
}
