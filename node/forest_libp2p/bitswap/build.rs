// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use protobuf_codegen::Customize;
use walkdir::WalkDir;

const PROTO_DIR: &str = "proto";
const CARGO_OUT_DIR: &str = "proto";

fn main() -> anyhow::Result<()> {
    protobuf_codegen::Codegen::new()
        .pure()
        .cargo_out_dir(CARGO_OUT_DIR)
        .inputs(get_inputs()?.as_slice())
        .include(PROTO_DIR)
        .customize(Customize::default().lite_runtime(true))
        .run()?;
    Ok(())
}

fn get_inputs() -> anyhow::Result<Vec<PathBuf>> {
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
