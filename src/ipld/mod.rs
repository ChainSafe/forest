// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cid_hashset;
mod error;
pub mod json;
pub mod selector;
pub mod util;

pub use libipld::Path;
pub use libipld_core::ipld::Ipld;
pub use util::*;

pub use self::{cid_hashset::CidHashSet, error::Error};

fn lookup_segment<'a>(ipld: &'a Ipld, segment: &str) -> Option<&'a Ipld> {
    match ipld {
        Ipld::Map(map) => map.get(segment),
        Ipld::List(list) => list.get(segment.parse::<usize>().ok()?),
        _ => None,
    }
}

pub use libipld_core::serde::{from_ipld, to_ipld};
