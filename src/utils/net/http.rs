// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use once_cell::sync::Lazy;

pub fn global_http_client() -> reqwest::Client {
    static CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);
    CLIENT.clone()
}
