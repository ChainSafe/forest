// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

fn main() {
    // whitelist the cfg for cargo clippy
    println!("cargo::rustc-check-cfg=cfg(f3sidecar)");

    // Do not build f3-sidecar on docs.rs publishing
    // No proper version of Go compiler is available.
    if !is_docs_rs() && is_sidecar_ffi_enabled() {
        println!("cargo:rustc-cfg=f3sidecar");
        println!("cargo::rerun-if-changed=f3-sidecar");
        unsafe {
            std::env::set_var("GOWORK", "off");
            // `Netgo` is enabled for all the platforms to be consistent across different builds. It
            // is using pure Go implementation for functionality like name resolution. In the case of
            // sidecar it does not make much difference, but it does fix the Apple silicons builds.
            // See <https://github.com/status-im/status-mobile/issues/20135#issuecomment-2137400475>
            std::env::set_var("GOFLAGS", "-tags=netgo");
        }
        rust2go::Builder::default()
            .with_go_src("./f3-sidecar")
            // the generated Go file has been commited to the git repository,
            // uncomment to regenerate the code locally
            // .with_regen_arg(rust2go::RegenArgs {
            //     src: "./src/f3/go_ffi.rs".into(),
            //     dst: "./f3-sidecar/ffi_gen.go".into(),
            //     without_main: true,
            //     ..Default::default()
            // })
            .build();
    }
}

// See <https://docs.rs/about/builds#detecting-docsrs>
fn is_docs_rs() -> bool {
    std::env::var("DOCS_RS").is_ok()
}

fn is_sidecar_ffi_enabled() -> bool {
    // Opt-out building the F3 sidecar staticlib
    match std::env::var("FOREST_F3_SIDECAR_FFI_BUILD_OPT_OUT") {
        Ok(value) => !matches!(value.to_lowercase().as_str(), "1" | "true"),
        _ => true,
    }
}
