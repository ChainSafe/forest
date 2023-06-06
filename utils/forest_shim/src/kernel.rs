// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm3::kernel::ExecutionError as ExecutionError_v3;

pub struct ExecutionError(ExecutionError_v3);

impl std::fmt::Display for ExecutionError {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
