// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

fn main() {
    println!("cargo::rerun-if-changed=src/tests/go_app");
    std::env::set_var("GOWORK", "off");
    std::env::set_var("GOFLAGS", "-tags=netgo");
    rust2go::Builder::default()
        .with_go_src("./src/tests/go_app")
        .with_regen_arg(rust2go::RegenArgs {
            src: "./src/tests/go_ffi.rs".into(),
            dst: "./src/tests/go_app/gen.go".into(),
            ..Default::default()
        })
        .build();
}
