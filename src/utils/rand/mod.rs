// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use rand::{CryptoRng, Rng, RngCore, SeedableRng as _};

/// A wrapper of [`rand::thread_rng`] that can be overridden by reproducible seeded
/// [`rand_chacha::ChaChaRng`] via `FOREST_TEST_RNG_FIXED_SEED` environment variable.
/// This is required for reproducible test cases for normally non-deterministic methods.
pub fn forest_rng() -> impl Rng + CryptoRng {
    const ENV_KEY: &str = "FOREST_TEST_RNG_FIXED_SEED";
    forest_rng_internal(ENV_KEY, false)
}

/// A wrapper of [`rand::rngs::OsRng`] that can be overridden by reproducible seeded
/// [`rand_chacha::ChaChaRng`] via `FOREST_TEST_RNG_FIXED_SEED` environment variable.
/// This is required for reproducible test cases for normally non-deterministic methods.
pub fn forest_os_rng() -> impl Rng + CryptoRng {
    const ENV_KEY: &str = "FOREST_TEST_OS_RNG_FIXED_SEED";
    forest_rng_internal(ENV_KEY, true)
}

fn forest_rng_internal(
    env_key: &str,
    prefer_os_rng_for_enhanced_security: bool,
) -> impl Rng + CryptoRng {
    if let Ok(v) = std::env::var(env_key) {
        if let Ok(seed) = v.parse() {
            tracing::warn!("[security] using test RNG with fixed seed {seed} set by {env_key}");
            return Either::Left(rand_chacha::ChaChaRng::seed_from_u64(seed));
        } else {
            tracing::warn!(
                "invalid u64 seed set by {env_key}: {v}. Falling back to the default RNG."
            );
        }
    }
    if prefer_os_rng_for_enhanced_security {
        Either::Right(Either::Left(rand::rngs::OsRng))
    } else {
        Either::Right(Either::Right(rand::thread_rng()))
    }
}

enum Either<A, B> {
    Left(A),
    Right(B),
}

impl<A, B> RngCore for Either<A, B>
where
    A: RngCore,
    B: RngCore,
{
    fn next_u32(&mut self) -> u32 {
        match self {
            Self::Left(i) => i.next_u32(),
            Self::Right(i) => i.next_u32(),
        }
    }

    fn next_u64(&mut self) -> u64 {
        match self {
            Self::Left(i) => i.next_u64(),
            Self::Right(i) => i.next_u64(),
        }
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        match self {
            Self::Left(i) => i.fill_bytes(dst),
            Self::Right(i) => i.fill_bytes(dst),
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        match self {
            Self::Left(i) => i.try_fill_bytes(dest),
            Self::Right(i) => i.try_fill_bytes(dest),
        }
    }
}

impl<A, B> CryptoRng for Either<A, B>
where
    A: RngCore,
    B: RngCore,
{
}
