// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

fn main() {
    rust2go::Builder::new()
        .with_go_src("./src/libp2p/tests/go-kad")
        .with_regen_arg(rust2go::RegenArgs {
            src: "./src/libp2p/tests/kad_go_compat/kad_ffi.rs".into(),
            dst: "./src/libp2p/tests/go-kad/gen.go".into(),
            ..Default::default()
        })
        .build();
}
