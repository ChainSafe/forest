use std::fmt;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::de;

pub struct OptionalDateTimeFromCustomFormatVisitor;

impl<'de> de::Visitor<'de> for OptionalDateTimeFromCustomFormatVisitor {
    type Value = Option<DateTime<Utc>>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "null or a datetime string")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(None)
    }

    fn visit_some<D>(self, d: D) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        Ok(Some(d.deserialize_str(DateTimeFromCustomFormatVisitor)?))
    }
}

struct DateTimeFromCustomFormatVisitor;

pub fn deserialize<'de, D>(d: D) -> Result<DateTime<Utc>, D::Error>
where
    D: de::Deserializer<'de>,
{
    d.deserialize_str(DateTimeFromCustomFormatVisitor)
}

impl<'de> de::Visitor<'de> for DateTimeFromCustomFormatVisitor {
    type Value = DateTime<Utc>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a datetime string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match NaiveDateTime::parse_from_str(value, "%d-%b-%Y %H:%M:%S") {
            Ok(ndt) => Ok(DateTime::from_utc(ndt, Utc)),
            Err(e) => Err(E::custom(format!("Parse error {} for {}", e, value))),
        }
    }
}
