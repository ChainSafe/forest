// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! ```console
//! $ cargo bench --features="benchmark-private" --bench serde-bench
//! ```

use criterion::{criterion_group, criterion_main};
use forest::benchmark_private::serde::bench_serde;

criterion_group!(benches, bench_serde);
criterion_main!(benches);
