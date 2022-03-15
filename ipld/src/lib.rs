// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod error;
mod path;
mod path_segment;
pub mod selector;
pub mod util;

#[cfg(feature = "json")]
pub mod json;

#[macro_use]
mod macros;

pub use self::error::Error;
pub use path::Path;
pub use path_segment::PathSegment;
pub use util::*;

pub use libipld_core::ipld::Ipld;

fn lookup_segment<'a>(ipld: &'a Ipld, segment: &PathSegment) -> Option<&'a Ipld> {
    match ipld {
        Ipld::Map(map) => match segment {
            PathSegment::String(s) => map.get(s),
            PathSegment::Int(i) => map.get(&i.to_string()),
        },
        Ipld::List(list) => list.get(segment.to_index()?),
        _ => None,
    }
}

pub use libipld_core::serde::from_ipld;
pub use libipld_core::serde::to_ipld;
