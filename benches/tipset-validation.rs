// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! ```console
//! $ cargo bench --features="benchmark-private" --bench tipset-validation
//! ```

use criterion::{criterion_group, criterion_main};
use forest::benchmark_private::tipset_validation::bench_tipset_validation;

criterion_group!(benches, bench_tipset_validation);
criterion_main!(benches);
