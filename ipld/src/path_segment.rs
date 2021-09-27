// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::de;
use std::convert::TryFrom;
use std::fmt;

/// Represents either a key in a map or an index in a list.
#[derive(Clone, Debug, PartialEq)]
pub enum PathSegment {
    /// Key in a map
    String(String),
    /// Index in a list
    Int(usize),
}

impl PathSegment {
    /// Return index or conversion from string to index.
    /// If path segment is a String and cannot be converted, None is returned.
    pub fn to_index(&self) -> Option<usize> {
        match self {
            PathSegment::String(s) => s.parse().ok(),
            PathSegment::Int(i) => Some(*i),
        }
    }
}

impl From<usize> for PathSegment {
    fn from(i: usize) -> Self {
        Self::Int(i)
    }
}

impl From<String> for PathSegment {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&str> for PathSegment {
    fn from(s: &str) -> Self {
        // Try to parse as usize to avoid heap allocations.
        // (Int and String segments are handled the same in traversals)
        match s.parse::<usize>() {
            Ok(u) => PathSegment::Int(u),
            Err(_) => PathSegment::String(s.to_owned()),
        }
    }
}

impl fmt::Display for PathSegment {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PathSegment::String(s) => write!(f, "{}", s),
            PathSegment::Int(i) => write!(f, "{}", i),
        }
    }
}

impl<'de> de::Deserialize<'de> for PathSegment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct PathSegmentVisitor;
        impl<'de> de::Visitor<'de> for PathSegmentVisitor {
            type Value = PathSegment;
            fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt.write_str("a string or a usize")
            }
            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PathSegment::String(value))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_string(v.to_owned())
            }
            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PathSegment::Int(
                    usize::try_from(v).map_err(de::Error::custom)?,
                ))
            }
        }
        deserializer.deserialize_any(PathSegmentVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_segment_from_string() {
        let seg: PathSegment = "12".into();
        assert_eq!(seg, PathSegment::Int(12));
        assert_eq!(seg.to_string(), "12");
    }
}
