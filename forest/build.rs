// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::process::Command;

#[cfg(not(feature = "release"))]
const RELEASE_TRACK: &str = "unstable";

#[cfg(feature = "release")]
const RELEASE_TRACK: &str = "alpha";

fn main() {
    // expose environment variable FOREST_VERSION at build time
    println!("cargo:rustc-env=FOREST_VERSION={}", version());
}

// returns version string at build time, e.g., `v0.1.0/unstable/7af2f5bf`
fn version() -> String {
    let git_hash = match Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output()
    {
        Ok(output) => String::from_utf8(output.stdout).unwrap_or_default(),
        _ => "unknown".to_owned(),
    };
    format!(
        "v{}/{}/{}",
        env!("CARGO_PKG_VERSION"),
        RELEASE_TRACK,
        git_hash,
    )
}
