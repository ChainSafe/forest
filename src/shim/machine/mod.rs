// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm2::machine::MultiEngine as MultiEngine_v2;
use fvm3::engine::MultiEngine as MultiEngine_v3;
use fvm4::engine::MultiEngine as MultiEngine_v4;
mod manifest;
pub use manifest::*;

pub struct MultiEngine {
    pub v2: MultiEngine_v2,
    pub v3: MultiEngine_v3,
    pub v4: MultiEngine_v4,
}

impl Default for MultiEngine {
    fn default() -> MultiEngine {
        MultiEngine::new(std::thread::available_parallelism().map(|x| x.get() as u32))
    }
}

impl MultiEngine {
    pub fn new(concurrency: Result<u32, std::io::Error>) -> MultiEngine {
        let concurrency = concurrency.ok();
        MultiEngine {
            v2: Default::default(),
            v3: concurrency.map_or_else(Default::default, MultiEngine_v3::new),
            v4: concurrency.map_or_else(Default::default, MultiEngine_v4::new),
        }
    }
}
