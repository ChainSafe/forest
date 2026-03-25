// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// Calculate the probability mass function (PMF) of a Skellam distribution.
/// Ported from <https://github.com/jsoares/rusty-skellam/blob/main/src/lib.rs>
///
/// The Skellam distribution is the probability distribution of the difference
/// of two independent Poisson random variables.
///
/// # Arguments
///
/// * `k` - The difference of two Poisson random variables.
/// * `mu1` - The expected value of the first Poisson distribution.
/// * `mu2` - The expected value of the second Poisson distribution.
///
/// # Returns
///
/// * A `f64` representing the PMF of the Skellam distribution at `k`.
///
pub(super) fn skellam_pmf(k: f64, mu1: f64, mu2: f64) -> f64 {
    if mu1.is_nan() || mu1 <= 0.0 || mu2.is_nan() || mu2 <= 0.0 {
        return f64::NAN;
    }
    let bessel_i = puruspe::bessel::In(k.abs() as u32, 2.0 * (mu1 * mu2).sqrt());
    (-mu1 - mu2).exp() * (mu1 / mu2).powf(k / 2.0) * bessel_i
}

#[cfg(test)]
mod tests {
    use super::*;
    use all_asserts::*;
    use rstest::rstest;

    // Ported from <https://github.com/rvagg/go-skellam-pmf/blob/9cf38e94a40e4bbf2646ac13717072a05a4b1734/skellam_test.go#L45>.
    // It validates SkellamPMF against the direct
    // Poisson convolution sum. The convolution is an independent computation path
    // (no Bessel functions) and is reliable for moderate parameters.
    #[rstest]
    #[case(0, 1.5, 3.5)]
    #[case(1, 1.5, 3.5)]
    #[case(-1, 1.5, 3.5)]
    #[case(-3, 1.5, 3.5)]
    #[case(5, 1.5, 3.5)]
    #[case(0, 0.3, 0.7)]
    #[case(1, 0.3, 0.7)]
    #[case(-2, 0.3, 0.7)]
    #[case(0, 5.0, 5.0)]
    #[case(3, 5.0, 5.0)]
    #[case(-3, 5.0, 5.0)]
    #[case(0, 15.0, 35.0)]
    #[case(5, 15.0, 35.0)]
    #[case(-10, 15.0, 35.0)]
    #[case(-20, 15.0, 35.0)]
    #[case(0, 75.0, 175.0)]
    #[case(10, 75.0, 175.0)]
    #[case(-50, 75.0, 175.0)]
    #[case(0, 15.0, 8.0)]
    #[case(7, 15.0, 8.0)]
    fn test_skellam_pmf_against_convolution(#[case] k: i64, #[case] mu1: f64, #[case] mu2: f64) {
        let got = skellam_pmf(k as f64, mu1, mu2);
        let want = poisson_convolution_pmf(k, mu1, mu2);
        let relative_error = (got - want).abs() / want.abs();
        assert_lt!(
            relative_error,
            1e-10,
            "Skellam PMF does not match convolution for k={k}, mu1={mu1}, mu2={mu2}, got={got}, want={want}, relative_error={relative_error}"
        );
    }

    // Ported from <https://github.com/rvagg/go-skellam-pmf/blob/9cf38e94a40e4bbf2646ac13717072a05a4b1734/skellam_test.go#L112>.
    // It verifies that the PMF values sum to ~1 over a
    // sufficient range. This is a structural property of any valid PMF.
    #[rstest]
    #[case(1.5, 3.5, -30, 30)]
    #[case(15.0, 35.0, -60, 20)]
    #[case(5.0, 5.0, -20, 20)]
    #[case(15.0, 8.0, -20, 40)]
    fn test_skellam_pmf_sums_to_one(
        #[case] mu1: f64,
        #[case] mu2: f64,
        #[case] k_min: i64,
        #[case] k_max: i64,
    ) {
        let mut total = 0.0;
        for k in k_min..=k_max {
            total += skellam_pmf(k as f64, mu1, mu2);
        }
        assert_lt!(
            (total - 1.0).abs(),
            1e-6,
            "sum over [{k_min}, {k_max}] = {total}, want ~1.0"
        );
    }

    #[rstest]
    #[case(0, -1.0, 1.0)]
    #[case(0, 1.0, -1.0)]
    #[case(0, 0.0, 1.0)]
    #[case(0, f64::NAN, 1.0)]
    fn test_skellam_pmf_invalid_inputs(#[case] k: i64, #[case] mu1: f64, #[case] mu2: f64) {
        assert!(
            skellam_pmf(k as f64, mu1, mu2).is_nan(),
            "Expected NaN for invalid inputs k={k}, mu1={mu1}, mu2={mu2}"
        );
    }

    /// computes the Skellam PMF from its definition as a
    /// convolution of two Poisson distributions:
    ///
    /// `P(K=k) = sum_{j=max(0,-k)}^{inf} Poisson(j+k, mu1) * Poisson(j, mu2)`
    ///
    /// This is numerically reliable for moderate parameters and serves as an
    /// independent reference implementation (no Bessel functions involved).
    fn poisson_convolution_pmf(k: i64, mu1: f64, mu2: f64) -> f64 {
        let mut j_start = 0;
        if k < 0 {
            j_start = -k;
        }

        let mut total = 0.0;
        for j in j_start..(j_start + 2000) {
            let log_p1 = poisson_log_pmf((j + k) as f64, mu1);
            let log_p2 = poisson_log_pmf(j as f64, mu2);
            let term = (log_p1 + log_p2).exp();
            total += term;
            if j > j_start + 10 && term < total * 1e-16 {
                break;
            }
        }
        total
    }

    fn poisson_log_pmf(k: f64, lambda: f64) -> f64 {
        assert!(k >= 0., "k should not be negative");
        let lg = libm::lgamma(k + 1.);
        k * libm::log(lambda) - lambda - lg
    }
}
