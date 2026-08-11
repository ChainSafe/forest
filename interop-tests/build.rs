// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

fn main() {
    println!("cargo::rerun-if-changed=src/tests/go_app");
    println!("cargo::rerun-if-changed=src/tests/go_ffi.rs");
    println!("cargo::rerun-if-env-changed=FOREST_REGENERATE_GO_FFI");

    unsafe {
        std::env::set_var("GOWORK", "off");
        std::env::set_var("GOFLAGS", "-tags=netgo");
    }
    
    let mut builder = rust2go::Builder::default()
        .with_go_src("./src/tests/go_app");

    // the generated Go file has been committed to the git repository
    // set the var to regenerate the file, CI sets this var to verify freshness.
    if std::env::var_os("FOREST_REGENERATE_GO_FFI").is_some() {
        builder = builder.with_regen_arg(rust2go::RegenArgs {
            src: "./src/tests/go_ffi.rs".into(),
            dst: "./src/tests/go_app/ffi_gen.go".into(),
            ..Default::default()
        })
    }

    builder.build();
}
