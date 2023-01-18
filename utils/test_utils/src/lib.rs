// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod chain_structures;

pub use self::chain_structures::*;

// Serialize macro used for testing
#[macro_export]
macro_rules! to_string_with {
    ($obj:expr, $serializer:path) => {{
        let mut writer = Vec::new();
        $serializer($obj, &mut serde_json::ser::Serializer::new(&mut writer)).unwrap();
        String::from_utf8(writer).unwrap()
    }};
}

// Deserialize macro used for testing
#[macro_export]
macro_rules! from_str_with {
    ($str:expr, $deserializer:path) => {
        $deserializer(&mut serde_json::de::Deserializer::from_str($str)).unwrap()
    };
}
