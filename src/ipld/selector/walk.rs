// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use cid::Cid;

use super::super::{Ipld, Path};

#[async_trait]
pub trait LinkResolver {
    /// Resolves a Cid link into it's respective IPLD node, if it exists.
    async fn load_link(&mut self, link: &Cid) -> Result<Option<Ipld>, String>;
}

#[async_trait]
impl LinkResolver for () {
    async fn load_link(&mut self, _link: &Cid) -> Result<Option<Ipld>, String> {
        Err("load_link not implemented on the LinkResolver for default implementation".into())
    }
}

/// Contains information about the last block that was traversed in walking of
/// the IPLD graph.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LastBlockInfo {
    pub path: Path,
    pub link: Cid,
}
