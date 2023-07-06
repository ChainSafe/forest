// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
use async_recursion::async_recursion;
use async_trait::async_trait;
use cid::Cid;

use super::super::{Ipld, Path};

#[cfg(test)]
use super::{
    super::{error::Error, lookup_segment},
    Selector,
};

#[cfg(test)]
impl Selector {
    /// Walks all nodes visited (not just matched nodes) and executes callback
    /// with progress and IPLD node. An optional link loader/resolver is
    /// passed in to be able to traverse links.
    pub async fn walk_all<L, F>(
        self,
        ipld: &Ipld,
        resolver: Option<L>,
        callback: F,
    ) -> Result<(), Error>
    where
        F: Fn(&Progress<L>, &Ipld, VisitReason) -> Result<(), String> + Sync,
        L: LinkResolver + Sync + Send,
    {
        Progress {
            resolver,
            path: Path::default(),
            last_block: None,
        }
        .walk_all(ipld, self, &callback)
        .await
    }

    /// Walks a graph of IPLD nodes, executing the callback only on the nodes
    /// "matched". If a resolver is passed in, links will be able to be
    /// traversed.
    pub async fn walk_matching<L, F>(
        self,
        ipld: &Ipld,
        resolver: Option<L>,
        callback: F,
    ) -> Result<(), Error>
    where
        F: Fn(&Progress<L>, &Ipld) -> Result<(), String> + Sync,
        L: LinkResolver + Sync + Send,
    {
        self.walk_all(ipld, resolver, |prog, ipld, reason| -> Result<(), String> {
            match reason {
                VisitReason::SelectionMatch => callback(prog, ipld),
                VisitReason::SelectionCandidate => Ok(()),
            }
        })
        .await
    }
}

/// Provides reason for callback in traversal for `walk_all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(unused)] // https://github.com/ChainSafe/forest/issues/3031
pub enum VisitReason {
    /// IPLD node visited was a specific match.
    SelectionMatch,
    /// IPLD node was visited while searching for matches.
    SelectionCandidate,
}

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

/// Contains progress of traversal and last block information from link
/// traversals.
#[derive(Debug, Default)]
#[cfg(test)]
pub struct Progress<L = ()> {
    resolver: Option<L>,
    path: Path,
    last_block: Option<LastBlockInfo>,
}

/// Contains information about the last block that was traversed in walking of
/// the IPLD graph.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LastBlockInfo {
    pub path: Path,
    pub link: Cid,
}

#[cfg(test)]
impl<L> Progress<L>
where
    L: LinkResolver + Sync + Send,
{
    /// Returns the path of the current progress
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the last block information from a link traversal.
    pub fn last_block(&self) -> Option<&LastBlockInfo> {
        self.last_block.as_ref()
    }

    #[async_recursion]
    async fn walk_all<F>(
        &mut self,
        ipld: &Ipld,
        selector: Selector,
        callback: &F,
    ) -> Result<(), Error>
    where
        F: Fn(&Progress<L>, &Ipld, VisitReason) -> Result<(), String> + Sync,
    {
        // Resolve any links transparently before traversing
        if let Ipld::Link(cid) = ipld {
            if let Some(resolver) = &mut self.resolver {
                self.last_block = Some(LastBlockInfo {
                    path: self.path.clone(),
                    link: *cid,
                });
                let mut node = resolver.load_link(cid).await.map_err(Error::Link)?;
                while let Some(Ipld::Link(c)) = node {
                    node = resolver.load_link(&c).await.map_err(Error::Link)?;
                }

                if let Some(n) = node {
                    return self.walk_all(&n, selector, callback).await;
                }
            }

            // Link did not resolve to anything, stop traversal
            return Ok(());
        }

        let reason = if selector.decide() {
            VisitReason::SelectionMatch
        } else {
            VisitReason::SelectionCandidate
        };
        callback(self, ipld, reason).map_err(Error::Custom)?;

        // If Ipld is list or map, continue traversal, otherwise return
        match ipld {
            Ipld::Map(_) | Ipld::List(_) => (),
            _ => return Ok(()),
        }

        match selector.interests() {
            Some(interests) => {
                for ps in interests {
                    let v = match lookup_segment(ipld, &ps) {
                        Some(ipld) => ipld,
                        None => continue,
                    };
                    self.traverse_node(ipld, selector.clone(), callback, &ps, v)
                        .await?;
                }
                Ok(())
            }
            None => {
                match ipld {
                    Ipld::Map(m) => {
                        for (k, v) in m.iter() {
                            self.traverse_node(ipld, selector.clone(), callback, k, v)
                                .await?;
                        }
                    }
                    Ipld::List(list) => {
                        for (i, v) in list.iter().enumerate() {
                            let ps = i.to_string();
                            self.traverse_node(ipld, selector.clone(), callback, &ps, v)
                                .await?;
                        }
                    }
                    _ => unreachable!(),
                }

                Ok(())
            }
        }
    }

    /// Utility function just to reduce duplicate logic. Can't do with a closure
    /// because async closures are currently unstable: <https://github.com/rust-lang/rust/issues/62290>
    async fn traverse_node<F>(
        &mut self,
        ipld: &Ipld,
        selector: Selector,
        callback: &F,
        ps: &str,
        v: &Ipld,
    ) -> Result<(), Error>
    where
        F: Fn(&Progress<L>, &Ipld, VisitReason) -> Result<(), String> + Sync,
    {
        if let Some(next_selector) = selector.explore(ipld, ps) {
            let prev = self.path.clone();
            self.path.join(ps);
            self.walk_all(v, next_selector, callback).await?;
            self.path = prev;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use cid::multihash::{Code::Blake2b256, MultihashDigest};
    use indexmap::IndexMap;
    use libipld_macro::ipld;

    use super::*;

    #[tokio::test]
    async fn basic_walk() {
        let selector = Selector::Matcher;

        selector
            .walk_matching::<(), _>(&ipld!("Some IPLD data!"), None, |_progress, ipld| {
                assert_eq!(ipld, &ipld!("Some IPLD data!"));
                Ok(())
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn explore_fields() {
        let selector = Selector::ExploreFields {
            fields: IndexMap::from([("name".to_owned(), Selector::Matcher)]),
        };
        let details = Cid::new_v1(fvm_ipld_encoding::DAG_CBOR, Blake2b256.digest(&[1, 2, 3]));
        let src = ipld!({"details": Ipld::Link(details), "name": "Test"});
        selector
            .walk_matching::<(), _>(&src, None, |_progress, ipld| {
                assert_eq!(&ipld!("Test"), ipld);
                Ok(())
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn explore_index() {
        let selector = Selector::ExploreIndex {
            index: 2,
            next: Box::new(Selector::Matcher),
        };
        let src = ipld!(["A", "B", "C", "D", "E"]);
        selector
            .walk_matching::<(), _>(&src, None, |_progress, ipld| {
                assert_eq!(&ipld!("C"), ipld);
                Ok(())
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn explore_range() {
        let selector = Selector::ExploreRange {
            start: 2,
            end: 4,
            next: Box::new(Selector::Matcher),
        };
        let src = ipld!(["A", "B", "C", "D", "E"]);
        selector
            .walk_matching::<(), _>(&src, None, |_progress, ipld| {
                assert!(&ipld!("C") == ipld || &ipld!("D") == ipld || &ipld!("E") == ipld);
                Ok(())
            })
            .await
            .unwrap();
    }
}
