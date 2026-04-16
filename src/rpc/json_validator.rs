// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! JSON validation utilities for RPC requests and responses processing.
//!
//! - **Duplicate key detection**: `serde_json` automatically deduplicates keys at parse time
//!   using a "last-write-wins" strategy. This means JSON like `{"/":"cid1", "/":"cid2"}` will
//!   keep only the last value, which can lead to unexpected behavior in RPC calls.
//! - **Unknown field detection**: `serde_json` silently ignores unknown fields by default.
//!   In strict mode, [`from_value_rejecting_unknown_fields`] applies to rpc request and
//!   responses.
//!
//! All of this is gated behind the `FOREST_STRICT_JSON` environment variable.

use ahash::HashSet;
use serde::de::DeserializeOwned;

pub const STRICT_JSON_ENV: &str = "FOREST_STRICT_JSON";

crate::def_is_env_truthy!(is_strict_mode, STRICT_JSON_ENV);

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

/// De-serializes a [`serde_json::Value`] into `T`, rejecting unknown fields when strict mode is
/// enabled. When strict mode is off, this is equivalent to [`serde_json::from_value`].
pub fn from_value_rejecting_unknown_fields<T: DeserializeOwned>(
    value: serde_json::Value,
) -> Result<T, serde_json::Error> {
    if !is_strict_mode() {
        return serde_json::from_value(value);
    }
    let mut unknown = Vec::new();
    let result: T = serde_ignored::deserialize(value, |path| {
        unknown.push(path.to_string());
    })?;
    if !unknown.is_empty() {
        return Err(serde::de::Error::custom(format!(
            "unknown field(s): {}. Set {STRICT_JSON_ENV}=0 to disable this check",
            unknown.join(", ")
        )));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;
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

    #[derive(Debug, Deserialize, PartialEq)]
    struct RpcTestReq {
        name: String,
        value: i32,
    }

    #[test]
    #[serial]
    fn test_unknown_fields_known_only() {
        with_strict_mode(true, || {
            let val = json!({"name": "alice", "value": 42});
            let result = from_value_rejecting_unknown_fields::<RpcTestReq>(val);
            assert_eq!(
                result.unwrap(),
                RpcTestReq {
                    name: "alice".into(),
                    value: 42
                }
            );
        });
    }

    #[test]
    #[serial]
    fn test_unknown_fields_detected() {
        with_strict_mode(true, || {
            let val = json!({"name": "alice", "value": 42, "extra": true});
            let err = from_value_rejecting_unknown_fields::<RpcTestReq>(val)
                .expect_err("expected Err when unknown JSON field is present under strict mode");
            let msg = err.to_string();
            assert!(
                msg.contains("unknown field(s)") && msg.contains("extra"),
                "got: {msg}"
            );
        });
    }

    #[test]
    #[serial]
    fn test_unknown_fields_strict_mode_off() {
        with_strict_mode(false, || {
            let val = json!({"name": "alice", "value": 42, "extra": true});
            let result = from_value_rejecting_unknown_fields::<RpcTestReq>(val);
            assert!(
                result.is_ok(),
                "unknown fields should be allowed when strict mode is off"
            );
        });
    }
}
