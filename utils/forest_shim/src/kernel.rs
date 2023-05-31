// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm::kernel::ExecutionError as ExecutionError_v2;

pub struct ExecutionError {
    pub execution_error_v2: ExecutionError_v2,
}
