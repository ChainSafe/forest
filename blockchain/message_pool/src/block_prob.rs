// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use statrs::function::gamma::ln_gamma;
use std::f64::consts::E;

const MAX_BLOCKS: usize = 15;
const MU: f64 = 5.0;

fn poiss_pdf(x: f64, mu: f64, cond: f64) -> f64 {
    let ln_gamma = ln_gamma(x + 1.0);
    let log_mu = mu.log(E);
    let exponent = log_mu * x - ln_gamma - cond;
    E.powf(exponent)
}

/// Calculate the number of winners for each block number, up to [MAX_BLOCKS].
// * This will be needed for optimal message selection
#[allow(dead_code)]
// TODO following two can be lazy_static
fn no_winners_prob() -> Vec<f64> {
    (0..MAX_BLOCKS)
        .map(|i| poiss_pdf(i as f64, MU, MU))
        .collect()
}

/// Calculate the number of winners for each block number, up to [MAX_BLOCKS], assuming at least
/// one winner.
fn no_winners_prob_assuming_more_than_one() -> Vec<f64> {
    let cond = (E.powf(5.0) - 1.0).log(E);
    (0..MAX_BLOCKS)
        .map(|i| poiss_pdf(i as f64, MU, cond))
        .collect()
}

fn binomial_coefficient(mut n: f64, k: f64) -> Result<f64, ()> {
    if k > n {
        return Err(());
    }

    let mut r = 1.0;
    let mut d = 1.0;
    while d <= k {
        r *= n;
        r /= d;
        n -= 1.0;
        d += 1.0;
    }
    Ok(r)
}

fn bino_pdf(x: f64, trials: f64, p: f64) -> f64 {
    // based on https://github.com/atgjack/prob
    if x > trials {
        return 0.0;
    }
    if p == 0.0 {
        if x == 0.0 {
            return 1.0;
        }
        return 0.0;
    }
    if (p - 1.0).abs() < f64::EPSILON {
        if (x - trials).abs() < f64::EPSILON {
            return 1.0;
        }
        return 0.0;
    }
    let coef = if let Ok(v) = binomial_coefficient(trials, x) {
        v
    } else {
        return 0.0;
    };

    let pow = p.powf(x) * (1.0 - p).powf(trials - x);
    coef * pow
}

pub fn block_probabilities(tq: f64) -> Vec<f64> {
    let no_winners = no_winners_prob_assuming_more_than_one();
    let p = 1.0 - tq;
    (0..MAX_BLOCKS)
        .map(|place| {
            no_winners
                .iter()
                .enumerate()
                .map(|(other_winner, p_case)| {
                    p_case * bino_pdf(place as f64, other_winner as f64, p)
                })
                .sum()
        })
        .collect()
}

#[test]
fn test_block_probability() {
    let bp = block_probabilities(1.0 - 0.15);
    for i in 0..bp.len() - 1 {
        assert!(bp[i] >= bp[i + 1]);
    }
}

#[test]
fn test_winner_probability() {
    use rand::thread_rng;
    use rand::Rng;
    let n = 1_000_000;
    let winner_prob = no_winners_prob();
    let mut sum = 0.0;

    // Generates a radnom number from 0 to not including 1
    let mut rng = thread_rng();

    for _ in 0..n {
        let mut miners_rand: f64 = rng.gen::<f64>() * f64::MAX;
        for j in 0..MAX_BLOCKS {
            miners_rand -= winner_prob[j];
            if miners_rand < 0.0 {
                break;
            }
            sum += 1.0;
        }
    }

    let avg = sum / (n as f64);
    assert!((avg - 5.0).abs() > 0.01, "Average too far off ");
}
