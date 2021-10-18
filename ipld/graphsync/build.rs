// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

extern crate protoc_rust;

fn main() {
    protoc_rust::Codegen::new()
        .out_dir("src/message/proto")
        .inputs(&["src/message/proto/message.proto"])
        .include("src/message/proto")
        .run()
        .expect("Running protoc failed.");
}
