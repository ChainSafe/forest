// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_types::build_version::version;

fn main() {
    // expose environment variable BINARY_VERSION at build time
    println!(
        "cargo:rustc-env=BINARY_VERSION={}",
        version(env!("CARGO_PKG_VERSION"))
    );
}
