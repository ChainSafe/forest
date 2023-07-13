// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm2::machine::MultiEngine as MultiEngine_v2;
use fvm3::engine::MultiEngine as MultiEngine_v3;
mod manifest;
pub use manifest::*;

pub struct MultiEngine {
    pub v2: MultiEngine_v2,
    pub v3: MultiEngine_v3,
}

impl Default for MultiEngine {
    fn default() -> MultiEngine {
        MultiEngine::new(std::thread::available_parallelism().map(|x| x.get() as u32))
    }
}

impl MultiEngine {
    pub fn new(concurrency: Result<u32, std::io::Error>) -> MultiEngine {
        MultiEngine {
            v2: MultiEngine_v2::new(),
            v3: MultiEngine_v3::new(concurrency.unwrap_or(1)), // `1` is default concurrency value in `fvm3`
        }
    }
}
