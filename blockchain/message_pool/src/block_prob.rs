// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use statrs::function::gamma::ln_gamma;
const MU: f64 = 5.0;
const MAX_BLOCKS: usize = 15;

// const NO_WINNERS_PROB: Vec<f64> = no_winners_prob();

fn no_winners_prob() -> Vec<f64>{
    // let  poiss_pdf  = |x: f64| -> f64 {
    //     let lg = ln_gamma(x + 1.0);
    //     ((MU.ln() * x) - lg - MU).exp()
    // };
    // let mut out = Vec::with_capacity(MAX_BLOCKS);
    // // for i in  0..MAX_BLOCKS {
    // //     out.push(poiss_pdf(i as f64));
    // // }
    // out
    todo!()
}

fn no_winners_prob_assuming_more_than_one() -> Vec<f64> {
    todo!()
}

fn binomial_coeff (n: f64, k: f64) -> f64 {
    todo!()
}

fn block_probabilities(tq: f64) -> f64 {
    todo!()
}