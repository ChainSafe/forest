// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(dead_code)]
use super::math::PRECISION;

use clock::ChainEpoch;
use encoding::Cbor;
use num_bigint::{bigint_ser, BigInt};
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct FilterEstimate {
    #[serde(with = "bigint_ser")]
    pub pos: BigInt,
    #[serde(with = "bigint_ser")]
    pub velo: BigInt,
}

impl FilterEstimate {
    pub fn new(pos: BigInt, velo: BigInt) -> Self {
        FilterEstimate {
            pos: pos << PRECISION,
            velo: velo << PRECISION,
        }
    }

    pub fn estimate(&self) -> BigInt {
        &self.pos >> PRECISION
    }

    pub fn extrapolate(&self, delta: ChainEpoch) -> BigInt {
        let delta_t = BigInt::from(delta) << PRECISION;
        let pos = &self.pos << PRECISION;
        (&self.velo * delta_t) + pos
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
        let delta_x = (&delta_t * &self.prev_est.velo) >> PRECISION;
        let mut pos = delta_x + &self.prev_est.pos;

        let obs = obs << PRECISION;
        let residual = obs - &pos;
        let revision_x = (&self.alpha * &residual) >> PRECISION;
        pos += &revision_x;

        let revision_v = (residual * &self.beta) / delta_t;
        let velo = revision_v + &self.prev_est.velo;
        FilterEstimate { pos, velo }
    }
}
