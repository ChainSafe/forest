// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::util::math::PRECISION;
use clock::ChainEpoch;
use encoding::tuple::*;
use encoding::Cbor;
use num_bigint::{bigint_ser, BigInt};

#[derive(Default, Serialize_tuple, Deserialize_tuple)]
pub struct FilterEstimate {
    #[serde(with = "bigint_ser")]
    pub position: BigInt,
    #[serde(with = "bigint_ser")]
    pub velocity: BigInt,
}

impl FilterEstimate {
    /// Create a new filter estimate given two Q.0 format ints.
    pub fn new(position: BigInt, velocity: BigInt) -> Self {
        FilterEstimate {
            position: position << PRECISION,
            velocity: velocity << PRECISION,
        }
    }

    /// Returns the Q.0 position estimate of the filter
    pub fn estimate(&self) -> BigInt {
        &self.position >> PRECISION
    }

    /// Extrapolate filter "position" delta epochs in the future.
    pub fn extrapolate(&self, delta: ChainEpoch) -> BigInt {
        let delta_t = BigInt::from(delta) << PRECISION;
        let position = &self.position << PRECISION;
        (&self.velocity * delta_t) + position
    }
}

impl Cbor for FilterEstimate {}

#[derive(Default)]
pub struct AlphaBetaFilter {
    alpha: BigInt,
    beta: BigInt,
    prev_est: FilterEstimate,
}

impl AlphaBetaFilter {
    pub fn load_filter(prev_est: FilterEstimate, alpha: BigInt, beta: BigInt) -> Self {
        AlphaBetaFilter {
            alpha,
            beta,
            prev_est,
        }
    }

    pub fn next_estimate(&self, obs: BigInt, epoch_delta: ChainEpoch) -> FilterEstimate {
        let delta_t = BigInt::from(epoch_delta) << PRECISION;
        let delta_x = (&delta_t * &self.prev_est.velocity) >> PRECISION;
        let mut position = delta_x + &self.prev_est.position;

        let obs = obs << PRECISION;
        let residual = obs - &position;
        let revision_x = (&self.alpha * &residual) >> PRECISION;
        position += &revision_x;

        let revision_v = (residual * &self.beta) / delta_t;
        let velocity = revision_v + &self.prev_est.velocity;
        FilterEstimate { position, velocity }
    }
}
