// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Date and time types. The only module allowed to use the underlying date-time library, so it can be swapped in one place.
#![allow(clippy::disallowed_types)]

use anyhow::Context as _;
use jiff::{Span, SpanRound, Unit, fmt::friendly::SpanPrinter, tz::TimeZone};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Neg;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch, or 0 if the system clock is before it.
pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// An instant in time, serialized as an RFC 3339 string in UTC.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Timestamp(jiff::Timestamp);

impl JsonSchema for Timestamp {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Timestamp".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "format": "date-time" })
    }
}

impl Timestamp {
    #[cfg(test)]
    pub const UNIX_EPOCH: Self = Self(jiff::Timestamp::UNIX_EPOCH);

    pub fn now() -> Self {
        Self(jiff::Timestamp::now())
    }

    pub fn from_unix_secs(secs: i64) -> Option<Self> {
        jiff::Timestamp::from_second(secs).ok().map(Self)
    }

    pub fn unix_secs(self) -> i64 {
        self.0.as_second()
    }

    pub fn unix_millis(self) -> i64 {
        self.0.as_millisecond()
    }

    pub fn unix_micros(self) -> i64 {
        self.0.as_microsecond()
    }

    pub fn checked_add(self, duration: SignedDuration) -> Option<Self> {
        self.0.checked_add(duration.0).ok().map(Self)
    }

    /// Time elapsed since `earlier`, or [`None`] when `earlier` is later than `self`.
    pub fn duration_since(self, earlier: Self) -> Option<Duration> {
        Duration::try_from(self.0.duration_since(earlier.0)).ok()
    }

    /// The calendar date in UTC.
    pub fn utc_date(self) -> Date {
        Date(self.0.to_zoned(TimeZone::UTC).date())
    }

    /// Formats in UTC with `strftime`-style specifiers.
    pub fn strftime(self, format: &str) -> String {
        self.0.strftime(format).to_string()
    }

    /// Formats in the system time zone, e.g. `2026-11-18 13:00:00.5 +01:00`.
    pub fn to_local_string(self) -> String {
        self.0
            .to_zoned(TimeZone::system())
            .strftime("%Y-%m-%d %H:%M:%S%.f %:z")
            .to_string()
    }

    /// RFC 3339 in UTC without fractional seconds, e.g. `2026-11-18T12:00:00Z`.
    pub fn to_rfc3339_secs(self) -> String {
        format!("{:.0}", self.0)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// A span of time that may be negative, e.g. a token lifetime that has already elapsed.
#[derive(Clone, Copy, Debug)]
pub struct SignedDuration(jiff::SignedDuration);

impl SignedDuration {
    pub const fn from_secs(secs: i64) -> Self {
        Self(jiff::SignedDuration::from_secs(secs))
    }

    pub const fn from_hours(hours: i64) -> Self {
        Self(jiff::SignedDuration::from_hours(hours))
    }

    pub const fn as_secs(self) -> i64 {
        self.0.as_secs()
    }
}

impl Neg for SignedDuration {
    type Output = Self;

    fn neg(self) -> Self {
        Self(-self.0)
    }
}

/// A calendar date, displayed as `YYYY-MM-DD`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(jiff::civil::Date);

impl Date {
    /// Returns [`None`] for an invalid date.
    pub fn new(year: i16, month: i8, day: i8) -> Option<Self> {
        jiff::civil::Date::new(year, month, day).ok().map(Self)
    }

    pub fn year(self) -> i16 {
        self.0.year()
    }

    pub fn month(self) -> i8 {
        self.0.month()
    }

    pub fn day(self) -> i8 {
        self.0.day()
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// A time of day with second precision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time(jiff::civil::Time);

impl Time {
    /// Returns [`None`] for an invalid time.
    pub fn new(hour: i8, minute: i8, second: i8) -> Option<Self> {
        jiff::civil::Time::new(hour, minute, second, 0)
            .ok()
            .map(Self)
    }

    pub fn hour(self) -> i8 {
        self.0.hour()
    }

    pub fn minute(self) -> i8 {
        self.0.minute()
    }

    pub fn second(self) -> i8 {
        self.0.second()
    }
}

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
    let duration = span.to_duration(&jiff::Timestamp::now().to_zoned(TimeZone::UTC))?;
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
