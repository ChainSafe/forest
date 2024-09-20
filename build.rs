// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

fn main() {
    // whitelist the cfg for cargo clippy
    println!("cargo::rustc-check-cfg=cfg(f3sidecar)");

    // Do not build f3-sidecar on docs.rs publishing
    // No proper version of Go compiler is available.
    if !is_docs_rs() && is_sidecar_ffi_enabled() {
        println!("cargo:rustc-cfg=f3sidecar");
        std::env::set_var("GOWORK", "off");
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
    // Note: arm64 is disabled on MacOS for now as it's reported rust2go build does not work there
    if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        false
    } else {
        // Opt-out building the F3 sidecar staticlib
        match std::env::var("FOREST_F3_SIDECAR_FFI_BUILD_OPT_OUT") {
            Ok(value) => !matches!(value.to_lowercase().as_str(), "1" | "true"),
            _ => true,
        }
    }
}
