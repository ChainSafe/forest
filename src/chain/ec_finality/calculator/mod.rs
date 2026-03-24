// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Implements the `FRC-0089` EC finality calculator.
//!
//! The calculator computes an upper bound on the probability that a confirmed
//! tipset could be reorganized out of the canonical chain by an adversarial
//! fork, using observed chain data (block counts per epoch). Under healthy
//! network conditions (~5 blocks/epoch), the 2^-30 finality guarantee
//! (roughly one-in-a-billion chance of reorg) is typically achieved within
//! ~30 epochs (~15 minutes), compared to the static 900-epoch (~7.5 hour)
//! EC finality assumption which is based on worst-case network conditions.
//!
//! Reference: https://github.com/filecoin-project/FIPs/blob/master/FRCs/frc-0089.md
//! Python reference: https://github.com/consensus-shipyard/ec-finality-calculator

mod skellam;
#[cfg(test)]
mod tests;

use anyhow::Context as _;

// `BISECT_LOW` and `BISECT_HIGH` define the search range for the bisect algorithm
// that finds the epoch depth at which the finality guarantee is met. A low
// bound of 3 avoids evaluating trivially shallow depths; a high bound of
// 200 accommodates degraded chains that take longer to finalize.
#[allow(dead_code)]
pub const BISECT_LOW: i64 = 3;
#[allow(dead_code)]
pub const BISECT_HIGH: i64 = 200;

// the Filecoin mainnet expected block production rate.
#[allow(dead_code)]
pub const DEFAULT_BLOCKS_PER_EPOCH: f64 = 5.0;

// the standard Filecoin security assumption for adversarial mining power.
#[allow(dead_code)]
pub const DEFAULT_BYZANTINE_FRACTION: f64 = 0.3;

// the target reorg probability as a power of 2. 2^-30 (~one-in-a-billion) is the standard Filecoin finality guarantee.
#[allow(dead_code)]
pub const DEFAULT_SAFETY_EXPONENT: i64 = -30;

/// Computes the upper-bound probability that a confirmed
/// tipset could be reorganized out of the canonical chain. This is a Go port
/// of the Python reference implementation from `FRC-0089`(`finality_calc_validator.py`).
#[allow(dead_code)]
pub fn calc_validator_prob(
    chain: &[i64],
    finality: i64,
    blocks_per_epoch: f64,
    byzantine_fraction: f64,
    current_epoch: i64,
    target_epoch: i64,
) -> anyhow::Result<f64> {
    if current_epoch <= target_epoch || target_epoch < 0 || current_epoch >= chain.len() as i64 {
        return Ok(1.0);
    }

    const NELIGIBLE_THRESHOLD: f64 = 1e-25;

    let mut max_k_l = 400;
    let mut max_k_b = ((current_epoch - target_epoch) * (blocks_per_epoch as i64)) as usize;
    let mut max_k_m = 400;
    let max_im = 100;

    let rate_malicious_blocks = blocks_per_epoch * byzantine_fraction;
    let rate_honest_blocks = blocks_per_epoch - rate_malicious_blocks;

    // Compute L: adversarial lead distribution at target epoch
    let mut pr_l = vec![0.; max_k_l + 1];

    let mut pr_l_k_prev = 0.0;
    for (k, pr_l_k) in pr_l.iter_mut().enumerate() {
        let mut sum_expected_adversarial_blocks_i = 0.0;
        let mut sum_chain_blocks_i = 0;

        for chain_i in chain
            .get(((current_epoch - finality).max(0) as usize)..(target_epoch as usize))
            .context("unexpected slice indexing error 1")?
            .iter()
            .rev()
        {
            sum_expected_adversarial_blocks_i += rate_malicious_blocks;
            sum_chain_blocks_i += chain_i;
            let prl_i = poisson_prob(
                sum_expected_adversarial_blocks_i,
                (k as i64 + sum_chain_blocks_i) as f64,
            );
            *pr_l_k = prl_i.max(*pr_l_k);
        }
        if k > 1 && *pr_l_k < NELIGIBLE_THRESHOLD && *pr_l_k < pr_l_k_prev {
            max_k_l = k;
            pr_l.truncate(k + 1);
            break;
        }
        pr_l_k_prev = *pr_l_k;
    }

    *pr_l
        .get_mut(0)
        .context("unexpected slice indexing error 2")? += 1. - pr_l.iter().sum::<f64>();

    // Compute B: adversarial blocks during settlement period
    let mut pr_b = vec![0.; max_k_b + 1];
    let mut pr_b_k_prev = 0.0;
    for (k, pr_b_k) in pr_b.iter_mut().enumerate() {
        *pr_b_k = poisson_prob(
            ((current_epoch - target_epoch) as f64) * rate_malicious_blocks,
            k as f64,
        );

        if k > 1 && *pr_b_k < NELIGIBLE_THRESHOLD && *pr_b_k < pr_b_k_prev {
            max_k_b = k;
            pr_b.truncate(k + 1);
            break;
        }
        pr_b_k_prev = *pr_b_k;
    }

    // Compute M: adversarial mining advantage in the future (Skellam distribution)
    let pr_hgt_0 = 1.0 - poisson_prob(rate_honest_blocks, 0.0);

    let mut exp_z = 0.0;
    for k in 0..((4. * blocks_per_epoch) as usize) {
        let pmf = poisson_prob(rate_malicious_blocks, k as f64);
        exp_z += ((rate_honest_blocks + k as f64) / (2.0_f64.powf(k as f64))) * pmf;
    }

    let rate_public_chain = pr_hgt_0 * exp_z;

    let mut pr_m = vec![0.; max_k_m + 1];
    let mut pr_m_k_prev = 0.0;
    for (k, pr_m_k) in pr_m.iter_mut().enumerate() {
        for i in (1..=max_im).rev() {
            let prob_m_i = skellam::skellam_pmf(
                k as f64,
                f64::from(i) * rate_malicious_blocks,
                f64::from(i) * rate_public_chain,
            );
            if prob_m_i < NELIGIBLE_THRESHOLD && prob_m_i < *pr_m_k {
                break;
            }
            *pr_m_k = prob_m_i.max(*pr_m_k);
        }

        if k > 1 && *pr_m_k < NELIGIBLE_THRESHOLD && *pr_m_k < pr_m_k_prev {
            max_k_m = k;
            pr_m.truncate(k + 1);
            break;
        }
        pr_m_k_prev = *pr_m_k;
    }

    *pr_m
        .get_mut(0)
        .context("unexpected slice indexing error 3")? += 1. - pr_m.iter().sum::<f64>();

    // Compute reorg probability upper bound via convolution
    let cumsum_l = cumsum(&pr_l);
    let cumsum_b = cumsum(&pr_b);
    let cumsum_m = cumsum(&pr_m);

    let k = chain
        .get((target_epoch as usize)..(current_epoch as usize))
        .context("unexpected slice indexing error 4")?
        .iter()
        .sum();

    let mut sum_l_ge_k = *cumsum_l.last().context("cumsum_l should not be empty")?;
    if k > 0 {
        sum_l_ge_k -= *cumsum_l
            .get(max_k_l.min(k as usize - 1))
            .context("unexpected slice indexing error 5")?;
    }

    let mut double_sum = 0.0;

    for l in 0..k {
        let mut sum_b_ge_k_min_l = *cumsum_b.last().context("cumsum_b should not be empty")?;
        if k - l - 1 > 0 {
            sum_b_ge_k_min_l -= *cumsum_b
                .get(max_k_b.min((k - l - 1) as usize))
                .context("unexpected slice indexing error 6")?;
        }
        let pr_l_i = pr_l
            .get(max_k_l.min(l as usize))
            .context("unexpected slice indexing error 7")?;
        double_sum += *pr_l_i * sum_b_ge_k_min_l;

        for b in 0..(k - l) {
            let mut sum_m_ge_k_min_l_min_b =
                *cumsum_m.last().context("cumsum_m should not be empty")?;
            if k - l - b - 1 > 0 {
                sum_m_ge_k_min_l_min_b -= *cumsum_m
                    .get(max_k_m.min((k - l - b - 1) as usize))
                    .context("unexpected slice indexing error 8")?;
            }
            double_sum += *pr_l_i
                * *pr_b
                    .get(max_k_b.min(b as usize))
                    .context("unexpected slice indexing error 9")?
                * sum_m_ge_k_min_l_min_b
        }
    }

    let pr_error = sum_l_ge_k + double_sum;
    Ok(pr_error.min(1.))
}

/// Performs a bisect search to find the shallowest depth at
/// which the reorg probability drops below the given guarantee. Returns -1 if
/// the guarantee is not met within the search range.
#[allow(dead_code)]
pub fn find_threshold_depth(
    chain: &[i64],
    finality: i64,
    blocks_per_epoch: f64,
    byzantine_fraction: f64,
    guarantee: f64,
) -> anyhow::Result<i64> {
    let current_epoch = chain.len() as i64 - 1;
    let (mut low, mut high) = (BISECT_LOW, BISECT_HIGH.min(current_epoch));

    if low >= high {
        return Ok(-1);
    }

    let prob_low = calc_validator_prob(
        chain,
        finality,
        blocks_per_epoch,
        byzantine_fraction,
        current_epoch,
        current_epoch - low,
    )?;
    if prob_low < guarantee {
        return Ok(low);
    }

    let prob_high = calc_validator_prob(
        chain,
        finality,
        blocks_per_epoch,
        byzantine_fraction,
        current_epoch,
        current_epoch - high,
    )?;
    if prob_high > guarantee {
        return Ok(-1);
    }

    while low < high {
        let mid = (low + high) / 2;
        let prob = calc_validator_prob(
            chain,
            finality,
            blocks_per_epoch,
            byzantine_fraction,
            current_epoch,
            current_epoch - mid,
        )?;
        if prob < guarantee {
            high = mid
        } else {
            low = mid + 1;
        }
    }
    Ok(low)
}

fn poisson_prob(lambda: f64, x: f64) -> f64 {
    poisson_log_prob(lambda, x).exp()
}

fn poisson_log_prob(lambda: f64, x: f64) -> f64 {
    if x < 0. || x.floor() != x {
        return f64::NEG_INFINITY;
    }
    if lambda == 0. {
        if x == 0. {
            return 0.; // P(X=0 | lambda=0) = 1, log(1) = 0
        }
        return f64::NEG_INFINITY;
    }
    let lg = libm::lgamma(x.floor() + 1.);
    x * lambda.ln() - lambda - lg
}

fn cumsum(arr: &[f64]) -> Vec<f64> {
    let mut result = Vec::with_capacity(arr.len());
    let mut s = 0.0;
    for v in arr {
        s += v;
        result.push(s);
    }
    result
}
