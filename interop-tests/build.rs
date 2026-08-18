// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

fn main() {
    println!("cargo::rerun-if-changed=src/tests/go_app");
    println!("cargo::rerun-if-changed=src/tests/go_ffi.rs");
    println!("cargo::rerun-if-env-changed=FOREST_FFI_GO_REGENERATE");

    unsafe {
        std::env::set_var("GOWORK", "off");
        std::env::set_var("GOFLAGS", "-tags=netgo");
    }

    let mut builder = rust2go::Builder::default().with_go_src("./src/tests/go_app");

    // the generated Go file has been committed to the git repository
    // set the var to regenerate the file, CI sets this var to verify freshness.
    if is_env_truthy("FOREST_FFI_GO_REGENERATE") {
        builder = builder.with_regen_arg(rust2go::RegenArgs {
            src: "./src/tests/go_ffi.rs".into(),
            dst: "./src/tests/go_app/ffi_gen.go".into(),
            ..Default::default()
        })
    }

    builder.build();
}

fn is_env_truthy(env: &str) -> bool {
    std::env::var(env)
        .ok()
        .map(|var| matches!(var.to_lowercase().as_str(), "1" | "true" | "yes" | "_yes_"))
        .unwrap_or_default()
}
