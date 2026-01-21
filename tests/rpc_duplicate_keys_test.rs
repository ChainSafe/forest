// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest::rpc::json_validator::{validate_json_for_duplicates, STRICT_JSON_ENV};

// https://github.com/ChainSafe/forest/issues/6424
#[test]
fn test_issue_duplicate_cids() {
    unsafe {
        std::env::set_var(STRICT_JSON_ENV, "1");
    }

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
    assert!(result.is_err(), "Should detect duplicate '/' keys");
    let error = result.unwrap_err();
    assert!(error.contains("duplicate key"));
    assert!(error.contains("/"));

    unsafe {
        std::env::remove_var(STRICT_JSON_ENV);
    }
}

#[test]
fn test_correct_format_passes() {
    unsafe {
        std::env::set_var(STRICT_JSON_ENV, "1");
    }

    let json = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "Filecoin.ChainGetMessagesInTipset",
        "params": [[
            {"/":"bafy2bzacea43254b5x6c4l22ynpjfoct5qvabbbk2abcfspfcjkiltivrlyqi"},
            {"/":"bafy2bzacea4viqyaozpfk57lnemwufryb76llxzmebxc7it2rnssqz2ljdl6a"},
            {"/":"bafy2bzaceav6j67epppz5ib55v5ty26dhkq4jinbsizq2olb3azbzxvfmc73o"}
        ]]
    }"#;

    let result = validate_json_for_duplicates(json);
    assert!(result.is_ok(), "Correct format should pass validation");

    unsafe {
        std::env::remove_var(STRICT_JSON_ENV);
    }
}
