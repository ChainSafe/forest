// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::PathSegment;
use std::fmt;

/// Describes a series of steps across a tree or DAG of Ipld,
/// where each segment in the path is a map key or list index.
/// Path is used in describing progress in a traversal; and can
/// also be used as an instruction for traversing from one Ipld node to another.
///
/// Parsing implementation matches go-ipld-prime. Prefixed and suffixed "/" characters are
/// removed. Also multiple "/" characters will be collapsed.
///
/// # Examples
///
/// ```
/// # use forest_ipld::Path;
/// let mut path: Path = "some/path/1".into();
///
/// // Can append segments to the path
/// path.push(2.into());
/// assert_eq!(path.to_string(), "some/path/1/2");
///
/// // Or combine paths
/// path.extend(&"other/path".into());
/// assert_eq!(path.to_string(), "some/path/1/2/other/path");
/// ```
#[derive(Debug, PartialEq, Default, Clone)]
pub struct Path {
    segments: Vec<PathSegment>,
}

impl Path {
    pub fn new(segments: Vec<PathSegment>) -> Self {
        Self { segments }
    }

    /// Extend `Path` with another `Path` by cloning and appending `PathSegment`s to `segments`.
    pub fn extend(&mut self, other: &Path) {
        self.segments.extend_from_slice(&other.segments)
    }

    /// Returns slice of `PathSegment`s of the `Path`.
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }

    /// Pushes a `PathSegment` to the end of the `Path`.
    pub fn push(&mut self, seg: PathSegment) {
        self.segments.push(seg)
    }

    /// Pops a `PathSegment` from the end of the path.
    pub fn pop(&mut self) -> Option<PathSegment> {
        self.segments.pop()
    }
}

impl From<&str> for Path {
    fn from(s: &str) -> Self {
        let segments: Vec<PathSegment> = s
            .split('/')
            .filter(|s| !s.is_empty())
            .map(PathSegment::from)
            .collect();
        Self { segments }
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.segments.is_empty() {
            return Ok(());
        }

        write!(f, "{}", self.segments[0])?;
        for v in &self.segments[1..] {
            write!(f, "/{}", v)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PathSegment::*;

    #[test]
    fn path_with_extra_delimiters() {
        let path: Path = "/12/some///1/5.5/".into();
        assert_eq!(
            path.segments,
            vec![
                Int(12),
                String("some".to_owned()),
                Int(1),
                String("5.5".to_owned())
            ]
        );
        assert_eq!(path.to_string(), "12/some/1/5.5")
    }
}
