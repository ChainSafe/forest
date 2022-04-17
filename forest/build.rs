// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//use forest_git_utils::current_commit;
// Import Git Reference from Git or serialized file
use forest_git_utils::CURRENT_COMMIT;
use std::env;

#[cfg(not(feature = "release"))]
const RELEASE_TRACK: &str = "unstable";

#[cfg(feature = "release")]
const RELEASE_TRACK: &str = "alpha";

const NETWORK: &str = if cfg!(feature = "devnet") {
    "devnet"
} else if cfg!(feature = "interopnet") {
    "interopnet"
} else if cfg!(feature = "calibnet") {
    "calibnet"
} else {
    "mainnet"
};

fn main() {
    // Git Reference from Git or serialized file
    env::set_var("CURRENT_COMMIT", CURRENT_COMMIT.as_str());
    println!("cargo:rustc-env=CURRENT_COMMIT={}", CURRENT_COMMIT.as_str());
    // expose environment variable FOREST_VERSON at build time
    println!("cargo:rustc-env=FOREST_VERSION={}", version());
}

// returns version string at build time, e.g., `v0.1.0/unstable/mainnet/7af2f5bf`
fn version() -> String {
    let git_hash = match env::var("CURRENT_COMMIT") {
        Ok(cmt) => cmt,
        Err(_) => CURRENT_COMMIT.to_string(),
    };
    format!(
        "v{}/{}/{}/{}",
        env!("CARGO_PKG_VERSION"),
        RELEASE_TRACK,
        NETWORK,
        git_hash,
    )
}
