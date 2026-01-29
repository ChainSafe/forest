// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! JSON validation utilities for detecting duplicate keys before serde_json processing.
//!
//! serde_json automatically deduplicates keys at parse time using a "last-write-wins" strategy
//! This means JSON like `{"/":"cid1", "/":"cid2"}` will keep only the last value, which can lead to unexpected behavior in RPC calls.

use ahash::HashSet;

pub const STRICT_JSON_ENV: &str = "FOREST_STRICT_JSON";

#[cfg(not(test))]
use std::sync::LazyLock;

#[cfg(not(test))]
static STRICT_MODE: LazyLock<bool> =
    LazyLock::new(|| crate::utils::misc::env::is_env_truthy(STRICT_JSON_ENV));

#[inline]
pub fn is_strict_mode() -> bool {
    #[cfg(test)]
    {
        crate::utils::misc::env::is_env_truthy(STRICT_JSON_ENV)
    }
    #[cfg(not(test))]
    {
        *STRICT_MODE
    }
}

/// validates JSON for duplicate keys by parsing at the token level.
pub fn validate_json_for_duplicates(json_str: &str) -> Result<(), String> {
    if !is_strict_mode() {
        return Ok(());
    }

    fn check_value(value: &sonic_rs::Value) -> Result<(), String> {
        match value.as_ref() {
            sonic_rs::ValueRef::Object(obj) => {
                let mut seen = HashSet::default();
                for (key, value) in obj.iter() {
                    if !seen.insert(key) {
                        return Err(format!(
                            "duplicate key '{key}' in JSON object - this likely indicates malformed input. \
                            Set {STRICT_JSON_ENV}=0 to disable this check"
                        ));
                    }
                    check_value(value)?;
                }
                Ok(())
            }
            sonic_rs::ValueRef::Array(arr) => {
                for item in arr.iter() {
                    check_value(item)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    // defer to serde_json for invalid JSON
    let value: sonic_rs::Value = match sonic_rs::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    check_value(&value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn with_strict_mode<F>(enabled: bool, f: F)
    where
        F: FnOnce(),
    {
        let original = std::env::var(STRICT_JSON_ENV).ok();

        if enabled {
            unsafe {
                std::env::set_var(STRICT_JSON_ENV, "1");
            }
        } else {
            unsafe {
                std::env::remove_var(STRICT_JSON_ENV);
            }
        }

        f();

        unsafe {
            match original {
                Some(val) => std::env::set_var(STRICT_JSON_ENV, val),
                None => std::env::remove_var(STRICT_JSON_ENV),
            }
        }
    }

    #[test]
    #[serial]
    fn test_no_duplicates() {
        with_strict_mode(true, || {
            let json = r#"{"a": 1, "b": 2, "c": 3}"#;
            assert!(validate_json_for_duplicates(json).is_ok());
        });
    }

    #[test]
    #[serial]
    fn test_duplicate_keys_detected() {
        with_strict_mode(true, || {
            let json = r#"{"/":"cid1", "/":"cid2"}"#;
            let result = validate_json_for_duplicates(json);
            assert!(result.is_err(), "Should have detected duplicate key");
            assert!(result.unwrap_err().contains("duplicate key"));
        });
    }

    #[test]
    #[serial]
    fn test_strict_mode_disabled() {
        with_strict_mode(false, || {
            // should pass with strict mode disabled
            let json = r#"{"/":"cid1", "/":"cid2"}"#;
            assert!(validate_json_for_duplicates(json).is_ok());
        });
    }

    #[test]
    #[serial]
    fn test_duplicate_cid_keys() {
        with_strict_mode(true, || {
            let json = r#"{
                "jsonrpc": "2.0",
                "id": 1,
                "method": "Filecoin.ChainGetMessagesInTipset",
                "params": [[{
                    "/":"bafy2bzacea43254b5x6c4l22ynpjfoct5qvabbbk2abcfspfcjkiltivrlyqi",
                    "/":"bafy2bzacea4viqyaozpfk57lnemwufryb76llxzmebxc7it2rnssqz2ljdl6a",
                    "/":"bafy2bzaceav6j67epppz5ib55v5ty26dhkq4jinbsizq2olb3azbzxvfmc73o"
                }]]
            }"#;

            let result = validate_json_for_duplicates(json);
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("duplicate key '/'"));
        });
    }
}
