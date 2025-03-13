// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use rand::{CryptoRng, Rng, RngCore, SeedableRng as _};

/// A wrapper of [`uuid::Builder::from_random_bytes`] that uses [`forest_rng`] internally
pub fn new_uuid_v4() -> uuid::Uuid {
    let mut random_bytes = uuid::Bytes::default();
    forest_rng().fill(&mut random_bytes);
    uuid::Builder::from_random_bytes(random_bytes).into_uuid()
}

/// A wrapper of [`rand::thread_rng`] that can be overridden by reproducible seeded
/// [`rand_chacha::ChaChaRng`] via `FOREST_TEST_RNG_FIXED_SEED` environment variable.
/// This is required for reproducible test cases for normally non-deterministic methods.
pub fn forest_rng() -> impl Rng + CryptoRng {
    forest_rng_internal(ForestRngMode::ThreadRng)
}

/// A wrapper of [`rand::rngs::OsRng`] that can be overridden by reproducible seeded
/// [`rand_chacha::ChaChaRng`] via `FOREST_TEST_RNG_FIXED_SEED` environment variable.
/// This is required for reproducible test cases for normally non-deterministic methods.
pub fn forest_os_rng() -> impl Rng + CryptoRng {
    forest_rng_internal(ForestRngMode::OsRng)
}

pub const FIXED_RNG_SEED_ENV: &str = "FOREST_TEST_RNG_FIXED_SEED";

enum ForestRngMode {
    ThreadRng,
    OsRng,
}

fn forest_rng_internal(mode: ForestRngMode) -> impl Rng + CryptoRng {
    const ENV: &str = FIXED_RNG_SEED_ENV;
    if let Ok(v) = std::env::var(ENV) {
        if let Ok(seed) = v.parse() {
            tracing::warn!("[security] using test RNG with fixed seed {seed} set by {ENV}");
            return Either::Left(rand_chacha::ChaChaRng::seed_from_u64(seed));
        } else {
            tracing::warn!("invalid u64 seed set by {ENV}: {v}. Falling back to the default RNG.");
        }
    }
    match mode {
        #[allow(clippy::disallowed_methods)]
        ForestRngMode::ThreadRng => Either::Right(Either::Left(rand::thread_rng())),
        #[allow(clippy::disallowed_types)]
        ForestRngMode::OsRng => Either::Right(Either::Right(rand::rngs::OsRng)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_fixed_seed_env() {
        std::env::set_var(FIXED_RNG_SEED_ENV, "0");

        let mut a = [0; 1024];
        let mut b = [0; 1024];

        forest_rng().fill(&mut a);
        forest_rng().fill(&mut b);
        assert_eq!(a, b);

        forest_os_rng().fill(&mut a);
        forest_os_rng().fill(&mut b);
        assert_eq!(a, b);

        std::env::remove_var(FIXED_RNG_SEED_ENV);
    }

    #[test]
    #[serial]
    fn test_thread_rng() {
        std::env::remove_var(FIXED_RNG_SEED_ENV);
        let mut a = [0; 1024];
        forest_rng().fill(&mut a);
        let mut b = [0; 1024];
        forest_rng().fill(&mut b);
        assert_ne!(a, b);
    }

    #[test]
    #[serial]
    fn test_os_rng() {
        std::env::remove_var(FIXED_RNG_SEED_ENV);
        let mut a = [0; 1024];
        forest_os_rng().fill(&mut a);
        let mut b = [0; 1024];
        forest_os_rng().fill(&mut b);
        assert_ne!(a, b);
    }
}
