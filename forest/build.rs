// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::process::Command;

#[cfg(not(feature = "release"))]
const RELEASE_TRACK: &str = "unstable";

#[cfg(feature = "release")]
const RELEASE_TRACK: &str = "alpha";

fn main() {
    // expose environment variable FOREST_VERSON at build time
    println!("cargo:rustc-env=FOREST_VERSION={}", version());
}

// returns version string at build time, e.g., `v0.1.0/unstable/7af2f5bf`
fn version() -> String {
    let git_cmd = Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output()
        .expect("Git references should be available on a build system");
    let git_hash = String::from_utf8(git_cmd.stdout).unwrap_or_default();
    format!(
        "v{}/{}/{}",
        env!("CARGO_PKG_VERSION"),
        RELEASE_TRACK,
        git_hash
    )
}
