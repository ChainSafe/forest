// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use protoc_rust_grpc::Codegen;
fn main() {
    Codegen::new()
        .includes(&["proto", "proto/api-common-protos"])
        .out_dir("src/drand_api")
        .inputs(&["proto/api.proto", "proto/common.proto"])
        .rust_protobuf(true)
        .run()
        .expect("protoc-rust-grpc");
}
