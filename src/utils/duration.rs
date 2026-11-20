// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context as _;
use jiff::{Span, SpanRound, Timestamp, Unit, fmt::friendly::SpanPrinter, tz::TimeZone};
use std::time::Duration;

/// Formats a duration for humans, e.g. `1d 2h 3m 4s 5ms`.
pub fn format_duration(duration: Duration) -> String {
    let printer = SpanPrinter::new();
    match Span::try_from(duration)
        .and_then(|span| span.round(SpanRound::new().largest(Unit::Day).days_are_24_hours()))
    {
        Ok(span) => printer.span_to_string(&span),
        // Beyond the range of `Span`, fall back to hours as the largest unit.
        Err(_) => printer.unsigned_duration_to_string(&duration),
    }
}

/// Parses a human-friendly (`1h 30m`, `5seconds`, `2 months`) or ISO 8601 (`PT1H`) duration.
///
/// Calendar units are measured from the current UTC time. A month ends on the same day next month, or on its last day when that day does not exist (January 31 plus one month is February 28 in 2027).
pub fn parse_duration(s: &str) -> anyhow::Result<Duration> {
    if s == "0" {
        return Ok(Duration::ZERO);
    }
    let span: Span = s.parse()?;
    let duration = span.to_duration(&Timestamp::now().to_zoned(TimeZone::UTC))?;
    Duration::try_from(duration).context("duration must not be negative")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(Duration::ZERO, "0s")]
    #[case(Duration::from_millis(1500), "1s 500ms")]
    #[case(Duration::from_nanos(1_234_567_891_011), "20m 34s 567ms 891µs 11ns")]
    #[case(Duration::from_secs(90_061), "1d 1h 1m 1s")]
    #[case(Duration::MAX, "5124095576030431h 15s 999ms 999µs 999ns")]
    fn test_format_duration(#[case] duration: Duration, #[case] expected: &str) {
        assert_eq!(format_duration(duration), expected);
    }

    #[rstest]
    #[case("0", 0)]
    #[case("5seconds", 5)]
    #[case("1min", 60)]
    #[case("10m", 600)]
    #[case("1h30m", 5400)]
    #[case("2 days", 172_800)]
    #[case("1w", 604_800)]
    #[case("PT1H", 3600)]
    fn test_parse_duration(#[case] s: &str, #[case] expected_secs: u64) {
        assert_eq!(
            parse_duration(s).unwrap(),
            Duration::from_secs(expected_secs)
        );
    }

    #[rstest]
    #[case("")]
    #[case("-5m")]
    #[case("5m ago")]
    #[case("5 fortnights")]
    fn test_parse_duration_invalid(#[case] s: &str) {
        assert!(parse_duration(s).is_err());
    }
}
