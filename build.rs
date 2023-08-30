// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context as _;
use protobuf_codegen::Customize;
use std::path::PathBuf;
use walkdir::WalkDir;

const PROTO_DIR: &str = "proto";
const CARGO_OUT_DIR: &str = "proto";

fn main() -> anyhow::Result<()> {
    ensure_actor_bundle_includable()?;
    generate_protobuf_code()
}

fn ensure_actor_bundle_includable() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=assets");

    // There's a bit of complexity here because:
    // - We want users to `cargo install forest-filecoin`, which requires publishing the actor bundle to crates.io
    // - We want devs to use `git-lfs` for the actor bundle
    let check_bundle = || {
        let bundle_size = std::fs::metadata("assets/actor_bundles.car.zst")
            .context("bundle doesn't exist")?
            .len();
        anyhow::ensure!(
            bundle_size == 2_438_387,
            "downloaded bundle has the wrong size"
        ); // update me if the bundle changes
        anyhow::Ok(())
    };

    if check_bundle().is_ok() {
        return Ok(()); // already have the right bundle
    }

    println!("cargo:warning=fetching actor bundle with git-lfs");
    std::process::Command::new("git-lfs")
        .arg("pull")
        .status()
        .context("failed to exec git-lfs. Is it installed?")
        .and_then(|status| {
            anyhow::ensure!(status.success(), "git-lfs exited with code {status:?}");
            Ok(())
        })?;

    check_bundle()
}

fn generate_protobuf_code() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=proto");

    protobuf_codegen::Codegen::new()
        .pure()
        .cargo_out_dir(CARGO_OUT_DIR)
        .inputs(get_proto_inputs()?.as_slice())
        .include(PROTO_DIR)
        .customize(Customize::default().lite_runtime(true))
        .run()?;
    Ok(())
}

fn get_proto_inputs() -> anyhow::Result<Vec<PathBuf>> {
    let mut inputs = Vec::new();
    for entry in WalkDir::new(PROTO_DIR).into_iter().flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "proto" {
                    inputs.push(path.into());
                }
            }
        }
    }
    Ok(inputs)
}
