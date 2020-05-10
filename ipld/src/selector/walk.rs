// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::super::Path;
use super::Ipld;
use async_trait::async_trait;
use cid::Cid;
use std::error::Error as StdError;

#[async_trait]
pub trait LinkResolver {
    #[allow(unused_variables)]
    /// Resolves a Cid link into it's respective Ipld node.
    async fn load_link(&self, link: &Cid) -> Result<Ipld, Box<dyn StdError>> {
        Err("load_link not implemented on the LinkResolver".into())
    }
}

pub struct Progress<L>
where
    L: LinkResolver,
{
    _link_loader: Option<L>,
    _path: Path,
}
