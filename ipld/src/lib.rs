// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod error;
pub mod selector;
pub mod util;

#[cfg(feature = "json")]
pub mod json;

pub use self::error::Error;
pub use libipld::Path;
pub use util::*;

pub use libipld_core::ipld::Ipld;

fn lookup_segment<'a>(ipld: &'a Ipld, segment: &str) -> Option<&'a Ipld> {
    match ipld {
        Ipld::Map(map) => map.get(segment),
        Ipld::List(list) => list.get(segment.parse::<usize>().ok()?),
        _ => None,
    }
}

pub use libipld_core::serde::from_ipld;
pub use libipld_core::serde::to_ipld;
