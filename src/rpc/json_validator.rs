// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! JSON validation utilities for detecting duplicate keys before serde_json processing.
//!
//! serde_json automatically deduplicates keys at parse time using a "last-write-wins" strategy
//! This means JSON like `{"/":"cid1", "/":"cid2"}` will keep only the last value, which can lead to unexpected behaviour in RPC calls. 

use ahash::HashSet;

#[cfg(not(test))]
use std::sync::LazyLock;

pub const STRICT_JSON_ENV: &str = "FOREST_STRICT_JSON";

#[inline]
pub fn is_strict_mode() -> bool {
    #[cfg(test)]
    {
        std::env::var(STRICT_JSON_ENV)
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }
    
    #[cfg(not(test))]
    {
        static STRICT_MODE: LazyLock<bool> = LazyLock::new(|| {
            std::env::var(STRICT_JSON_ENV)
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
        });
        *STRICT_MODE
    }
}

/// validates JSON for duplicate keys by parsing at the token level.
pub fn validate_json_for_duplicates(json_str: &str) -> Result<(), String> {
    if !is_strict_mode() {
        return Ok(());
    }

    let mut object_stack: Vec<HashSet<String>> = Vec::new();
    
    let mut chars = json_str.chars();
    let mut current_key = String::new();
    let mut in_string = false;
    let mut escape_next = false;
    let mut expecting_key = false;
    
    while let Some(ch) = chars.next() {
        if in_string {
            if escape_next {
                escape_next = false;
                if expecting_key {
                    current_key.push('\\');
                    current_key.push(ch);
                }
                continue;
            }
            if ch == '\\' {
                escape_next = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
                if expecting_key {
                    if let Some(keys) = object_stack.last_mut() {
                        if !keys.insert(current_key.clone()) {
                            return Err(format!(
                                "duplicate key '{}' in JSON object - this likely indicates malformed input. \
                                Set {}=0 to disable this check",
                                current_key, STRICT_JSON_ENV
                            ));
                        }
                    }
                    current_key.clear();
                    expecting_key = false;
                }
                continue;
            }
            if expecting_key {
                current_key.push(ch);
            }
            continue;
        }
        
        match ch {
            '"' => {
                in_string = true;
            }
            '{' => {
                object_stack.push(HashSet::default());
                expecting_key = true;
            }
            '}' => {
                if !object_stack.is_empty() {
                    object_stack.pop();
                }
                expecting_key = false;
            }
            '[' => {
                expecting_key = false;
            }
            ']' => {
                expecting_key = false;
            }
            ':' => {}
            ',' => {
                if !object_stack.is_empty() {
                    expecting_key = true;
                }
            }
            c if c.is_whitespace() => {}
            _ => {}
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_strict_mode<F>(enabled: bool, f: F)
    where
        F: FnOnce(),
    {
        if enabled {
            unsafe { std::env::set_var(STRICT_JSON_ENV, "1"); }
        } else {
            unsafe { std::env::remove_var(STRICT_JSON_ENV); }
        }
        f();
        unsafe { std::env::remove_var(STRICT_JSON_ENV); }
    }

    #[test]
    fn test_no_duplicates() {
        with_strict_mode(true, || {
            let json = r#"{"a": 1, "b": 2, "c": 3}"#;
            assert!(validate_json_for_duplicates(json).is_ok());
        });
    }

    #[test]
    fn test_duplicate_keys_detected() {
        with_strict_mode(true, || {
            let json = r#"{"/":"cid1", "/":"cid2"}"#;
            let result = validate_json_for_duplicates(json);
            assert!(result.is_err(), "Should have detected duplicate key");
            assert!(result.unwrap_err().contains("duplicate key"));
        });
    }

    #[test]
    fn test_strict_mode_disabled() {
        with_strict_mode(false, || { // should pass with strict mode disabled
            let json = r#"{"/":"cid1", "/":"cid2"}"#;
            assert!(validate_json_for_duplicates(json).is_ok());
        });
    }

    #[test]
    fn test_original_issue_case() {
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
