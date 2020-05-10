// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::super::{Error, Ipld, Path, PathSegment};
use super::Selector;
use async_recursion::async_recursion;
use async_trait::async_trait;
use cid::Cid;

/// Defines result type for traversal functions. Always returns a boxed error;
type Result<T> = std::result::Result<T, Error>;

impl Selector {
    /// Walks all nodes visited (not just matched nodes) and executes callback with progress and
    /// Ipld node. An optional link loader/ resolver is passed in to be able to traverse links.
    pub async fn walk_all<F, L>(self, ipld: &Ipld, resolver: Option<L>, callback: F) -> Result<()>
    where
        F: Fn(&Progress<L>, &Ipld, VisitReason) -> Result<()> + Sync,
        L: LinkResolver + Sync + Send + Clone,
    {
        Progress {
            resolver,
            path: Path::default(),
        }
        .walk_all(ipld, self, &callback)
        .await
    }

    /// Walks a graph of Ipld nodes, executing the callback only on the nodes "matched".
    /// If a resolver is passed in, links will be able to be traversed.
    pub async fn walk_matching<F, L>(
        self,
        ipld: &Ipld,
        resolver: Option<L>,
        callback: F,
    ) -> Result<()>
    where
        F: Fn(&Progress<L>, &Ipld) -> Result<()> + Sync,
        L: LinkResolver + Sync + Send + Clone,
    {
        self.walk_all(ipld, resolver, |prog, ipld, reason| -> Result<()> {
            if let VisitReason::SelectionMatch = reason {
                return callback(prog, ipld);
            }
            Ok(())
        })
        .await
    }
}

/// Provides reason for callback in traversal for `walk_all`.
pub enum VisitReason {
    /// Ipld node visited was a specific match.
    SelectionMatch,
    /// Ipld node was visited while searching for matches.
    SelectionCandidate,
}

#[async_trait]
pub trait LinkResolver {
    #[allow(unused_variables)]
    /// Resolves a Cid link into it's respective Ipld node, if it exists.
    async fn load_link(&self, link: &Cid) -> Result<Option<Ipld>> {
        Err("load_link not implemented on the LinkResolver".into())
    }
}

pub struct Progress<L> {
    resolver: Option<L>,
    path: Path,
}

impl<L> Progress<L>
where
    L: LinkResolver + Sync + Send + Clone,
{
    #[async_recursion]
    async fn walk_all<F>(&mut self, ipld: &Ipld, selector: Selector, callback: &F) -> Result<()>
    where
        F: Fn(&Progress<L>, &Ipld, VisitReason) -> Result<()> + Sync,
    {
        if selector.decide() {
            callback(self, ipld, VisitReason::SelectionMatch)?;
        } else {
            callback(self, ipld, VisitReason::SelectionCandidate)?;
        }

        // If Ipld is list or map, continue traversal, otherwise return
        match ipld {
            Ipld::Map(_) | Ipld::List(_) => (),
            _ => return Ok(()),
        }

        match selector.interests() {
            Some(interests) => {
                for ps in interests {
                    let v = match ipld.lookup_segment(&ps) {
                        Some(ipld) => ipld,
                        None => continue,
                    };
                    if let Some(next_selector) = selector.clone().explore(ipld, &ps) {
                        self.path.append(ps);
                        // If node is a link, try to load and traverse
                        if let Ipld::Link(cid) = v {
                            // TODO determine if we need to store last block info
                            if let Some(resolver) = &self.resolver {
                                match resolver.load_link(cid).await? {
                                    Some(v) => self.walk_all(&v, next_selector, callback).await?,
                                    None => return Ok(()),
                                }
                            }
                        } else {
                            self.walk_all(v, next_selector, callback).await?
                        }
                    }
                }
                Ok(())
            }
            None => {
                match ipld {
                    Ipld::Map(m) => {
                        for (k, v) in m.iter() {
                            let ps: PathSegment = PathSegment::from(k.as_ref());
                            if let Some(next_selector) = selector.clone().explore(ipld, &ps) {
                                self.path.append(ps);

                                // If node is a link, try to load and traverse
                                if let Ipld::Link(cid) = v {
                                    // TODO determine if we need to store last block info
                                    if let Some(resolver) = &self.resolver {
                                        match resolver.load_link(cid).await? {
                                            Some(v) => {
                                                self.walk_all(&v, next_selector, callback).await?
                                            }
                                            None => return Ok(()),
                                        }
                                    }
                                } else {
                                    self.walk_all(v, next_selector, callback).await?
                                }
                            }
                        }
                    }
                    Ipld::List(list) => {
                        for (i, v) in list.iter().enumerate() {
                            let ps: PathSegment = i.into();
                            if let Some(next_selector) = selector.clone().explore(ipld, &ps) {
                                self.path.append(ps);

                                // If node is a link, try to load and traverse
                                if let Ipld::Link(cid) = v {
                                    // TODO determine if we need to store last block info
                                    if let Some(resolver) = &self.resolver {
                                        match resolver.load_link(cid).await? {
                                            Some(v) => {
                                                self.walk_all(&v, next_selector, callback).await?
                                            }
                                            None => return Ok(()),
                                        }
                                    }
                                } else {
                                    self.walk_all(v, next_selector, callback).await?
                                }
                            }
                        }
                    }
                    _ => unreachable!(),
                }

                Ok(())
            }
        }
    }

    // #[async_recursion]
    // async fn walk_iterate_all<F>(&mut self, ipld: &Ipld, selector: Selector, callback: &F) -> Result<()>
    // where
    //     F: Fn(&Progress<L>, &Ipld, VisitReason) -> Result<()> + Sync,
    // {
    //     todo!()
    // }
}
