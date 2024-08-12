// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

fn main() {
    rust2go::Builder::new()
        .with_go_src("./src/tests/kad_go_compat")
        .with_link(rust2go::LinkType::Static)
        .with_regen_arg(rust2go::RegenArgs {
            src: "./src/tests/kad_go_compat/kad_ffi.rs".into(),
            dst: "./src/tests/kad_go_compat/gen.go".into(),
            ..Default::default()
        })
        .build();
}
