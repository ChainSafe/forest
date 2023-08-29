// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{bail, Context};
use protobuf_codegen::Customize;
use std::path::PathBuf;
use walkdir::WalkDir;

const PROTO_DIR: &str = "proto";
const CARGO_OUT_DIR: &str = "proto";

fn main() -> anyhow::Result<()> {
    ensure_required_bins_installed()?;
    generate_protobuf_code()
}

fn ensure_required_bins_installed() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=assets");

    let is_bundle_size_correct = || -> anyhow::Result<bool> {
        let bundle_size = std::fs::metadata("assets/actor_bundles.car.zst")?.len();
        Ok(bundle_size == 2_438_387)
    };

    // If the bundle size is correct, we don't need to pull the bundle.
    // Don't bail on failure, e.g., in case the file is not present.
    // We can still get it.
    if let Ok(true) = is_bundle_size_correct() {
        return Ok(());
    }

    std::process::Command::new("git-lfs")
        .arg("pull")
        .status()
        .with_context(|| {
            anyhow::anyhow!(
                "failed to run git lfs pull. \
            Please ensure git-lfs is installed."
            )
        })?;

    if is_bundle_size_correct()? {
        Ok(())
    } else {
        bail!(
            "actor bundle size is incorrect. \
            Please run `git lfs pull` and try again. If this problem persists, \
            please open an issue."
        )
    }
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
