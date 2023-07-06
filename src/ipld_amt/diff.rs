// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Copyright 2021-2023 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::borrow::Borrow;

use anyhow::Context;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use serde::{de::DeserializeOwned, Serialize};

use super::node::{CollapsedNode, Link};

use super::*;

#[derive(Debug, Eq, PartialEq)]
pub enum ChangeType {
    Add,
    Remove,
    Modify,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Change<Old, New = Old> {
    pub key: u64,
    pub before: Option<Old>,
    pub after: Option<New>,
}

impl<Old, New> Change<Old, New> {
    pub fn change_type(&self) -> ChangeType {
        match (&self.before, &self.after) {
            (Some(_), Some(_)) => ChangeType::Modify,
            (Some(_), None) => ChangeType::Remove,
            (None, Some(_)) => ChangeType::Add,
            (None, None) => panic!("Invalid change type, before and after cannot be both None"),
        }
    }
}

struct NodeContext<'bs, BS> {
    pub height: u32,
    pub bit_width: u32,
    pub store: &'bs BS,
}

impl<'bs, BS> NodeContext<'bs, BS> {
    fn nodes_at_height(&self) -> u64 {
        nodes_for_height(self.bit_width, self.height)
    }
}

impl<'bs, V, BS> From<&'bs Amt<V, BS>> for NodeContext<'bs, BS> {
    fn from(value: &'bs Amt<V, BS>) -> Self {
        Self {
            height: value.height(),
            bit_width: value.bit_width(),
            store: &value.block_store,
        }
    }
}

/// Returns a set of changes that transform node `a` into node `b`.
/// Ported from <https://github.com/filecoin-project/go-amt-ipld/blob/master/diff.go#L41>
pub fn diff<Old, New, OldBS, NewBS>(
    prev_amt: &Amt<Old, OldBS>,
    curr_amt: &Amt<New, NewBS>,
) -> anyhow::Result<Vec<Change<Old, New>>>
where
    Old: Serialize + DeserializeOwned + Clone,
    New: Serialize + DeserializeOwned + Clone,
    OldBS: Blockstore,
    NewBS: Blockstore,
{
    if prev_amt.bit_width() != curr_amt.bit_width() {
        anyhow::bail!(
            "diffing AMTs with differing bitWidths not supported (prev={}, cur={})",
            prev_amt.bit_width(),
            curr_amt.bit_width()
        );
    }

    if prev_amt.count() == 0 && curr_amt.count() != 0 {
        add_all(&curr_amt.into(), &curr_amt.root.node, 0)
    } else if prev_amt.count() != 0 && curr_amt.count() == 0 {
        remove_all(&prev_amt.into(), &prev_amt.root.node, 0)
    } else {
        diff_node(
            &prev_amt.into(),
            &prev_amt.root.node,
            &curr_amt.into(),
            &curr_amt.root.node,
            0,
        )
    }
}

fn add_all<Old, New, BS>(
    ctx: &NodeContext<BS>,
    node: &Node<New>,
    offset: u64,
) -> anyhow::Result<Vec<Change<Old, New>>>
where
    New: Serialize + DeserializeOwned + Clone,
    BS: Blockstore,
{
    let mut changes = Vec::with_capacity(match &node {
        Node::Leaf { vals } => vals.len(),
        Node::Link { links } => links.len(),
    });
    node.for_each_while(ctx.store, ctx.height, ctx.bit_width, offset, &mut |i, x| {
        changes.push(Change {
            key: i,
            before: None,
            after: Some(x.clone()),
        });
        Ok(true)
    })?;

    Ok(changes)
}

fn remove_all<Old, New, BS>(
    ctx: &NodeContext<BS>,
    node: &Node<Old>,
    offset: u64,
) -> anyhow::Result<Vec<Change<Old, New>>>
where
    Old: Serialize + DeserializeOwned + Clone,
    BS: Blockstore,
{
    let mut changes = Vec::with_capacity(match &node {
        Node::Leaf { vals } => vals.len(),
        Node::Link { links } => links.len(),
    });
    node.for_each_while(ctx.store, ctx.height, ctx.bit_width, offset, &mut |i, x| {
        changes.push(Change {
            key: i,
            before: Some(x.clone()),
            after: None,
        });
        Ok(true)
    })?;

    Ok(changes)
}

fn diff_leaves<Old, New>(
    prev_node: &Node<Old>,
    curr_node: &Node<New>,
    offset: u64,
) -> anyhow::Result<Vec<Change<Old, New>>>
where
    Old: Serialize + DeserializeOwned + Clone,
    New: Serialize + DeserializeOwned + Clone,
{
    let prev_vals = match prev_node {
        Node::Leaf { vals } => vals,
        _ => {
            anyhow::bail!("The previous node is expected to be a leaf node, offset: {offset}")
        }
    };

    let curr_vals = match curr_node {
        Node::Leaf { vals } => vals,
        _ => {
            anyhow::bail!("The current node is expected to be a leaf node, offset: {offset}")
        }
    };

    anyhow::ensure!(
        prev_vals.len() == curr_vals.len(),
        "node leaves have different numbers of values, prev: {}, curr: {}",
        prev_vals.len(),
        curr_vals.len()
    );

    let mut changes = Vec::with_capacity(prev_vals.len());

    for (i, (prev_val, curr_val)) in prev_vals.iter().zip(curr_vals.iter()).enumerate() {
        let index = offset + i as u64;
        match (prev_val, curr_val) {
            (None, None) => continue,
            (None, Some(_)) | (Some(_), None) => changes.push(Change {
                key: index,
                before: prev_val.clone(),
                after: curr_val.clone(),
            }),
            (Some(prev_val), Some(curr_val)) => {
                if fvm_ipld_encoding::to_vec(&prev_val)? != fvm_ipld_encoding::to_vec(&curr_val)? {
                    changes.push(Change {
                        key: index,
                        before: Some(prev_val.clone()),
                        after: Some(curr_val.clone()),
                    });
                }
            }
        }
    }

    Ok(changes)
}

fn diff_node<Old, New, OldBS, NewBS>(
    prev_ctx: &NodeContext<OldBS>,
    prev_node: &Node<Old>,
    curr_ctx: &NodeContext<NewBS>,
    curr_node: &Node<New>,
    offset: u64,
) -> anyhow::Result<Vec<Change<Old, New>>>
where
    Old: Serialize + DeserializeOwned + Clone,
    New: Serialize + DeserializeOwned + Clone,
    OldBS: Blockstore,
    NewBS: Blockstore,
{
    if prev_ctx.height == 0 && curr_ctx.height == 0 {
        diff_leaves(prev_node, curr_node, offset)
    } else if curr_ctx.height > prev_ctx.height {
        let sub_count = curr_ctx.nodes_at_height();
        let links = match curr_node {
            Node::Link { links } => links,
            _ => anyhow::bail!("Node::Link expected"),
        };
        let mut changes = Vec::with_capacity(links.len());
        for (i, link) in links.iter().enumerate() {
            if let Some(link) = link {
                let sub_ctx = NodeContext {
                    height: curr_ctx.height - 1,
                    bit_width: curr_ctx.bit_width,
                    store: curr_ctx.store,
                };
                let sub_node = get_sub_node(link, &sub_ctx, curr_ctx.bit_width)?;
                let new_offset = offset + sub_count * i as u64;

                changes.append(&mut if i == 0 {
                    diff_node(prev_ctx, prev_node, &sub_ctx, sub_node.borrow(), new_offset)?
                } else {
                    add_all(&sub_ctx, sub_node.borrow(), new_offset)?
                });
            }
        }

        Ok(changes)
    } else if curr_ctx.height < prev_ctx.height {
        let sub_count = nodes_for_height(prev_ctx.bit_width, prev_ctx.height);
        let links = match prev_node {
            Node::Link { links } => links,
            _ => anyhow::bail!("Node::Link expected"),
        };
        let mut changes = Vec::with_capacity(links.len());
        for (i, link) in links.iter().enumerate() {
            if let Some(link) = link {
                let sub_ctx = NodeContext {
                    height: prev_ctx.height - 1,
                    bit_width: prev_ctx.bit_width,
                    store: prev_ctx.store,
                };
                let sub_node = get_sub_node(link, &sub_ctx, prev_ctx.bit_width)?;
                let new_offset = offset + sub_count * i as u64;

                changes.append(&mut if i == 0 {
                    diff_node(&sub_ctx, sub_node.borrow(), curr_ctx, curr_node, new_offset)?
                } else {
                    remove_all(&sub_ctx, sub_node.borrow(), new_offset)?
                });
            }
        }

        Ok(changes)
    } else {
        anyhow::ensure!(
            prev_ctx.height == curr_ctx.height,
            "comparing non-leaf nodes of unequal heights"
        );

        match (prev_node, curr_node) {
            (Node::Link { links: prev_links }, Node::Link { links: curr_links }) => {
                anyhow::ensure!(
                    prev_links.len() == curr_links.len(),
                    "nodes have different numbers of links"
                );

                let mut changes = Vec::with_capacity(prev_links.len());
                let sub_count = prev_ctx.nodes_at_height();

                for (i, (prev_link, curr_link)) in
                    prev_links.iter().zip(curr_links.iter()).enumerate()
                {
                    match (prev_link, curr_link) {
                        (None, None) => continue,
                        (Some(prev_link), None) => {
                            let sub_ctx = NodeContext {
                                bit_width: prev_ctx.bit_width,
                                height: prev_ctx.height - 1,
                                store: prev_ctx.store,
                            };
                            let sub_node = get_sub_node(prev_link, &sub_ctx, prev_ctx.bit_width)?;
                            let new_offset = offset + sub_count * i as u64;
                            changes.append(&mut remove_all(
                                &sub_ctx,
                                sub_node.borrow(),
                                new_offset,
                            )?);
                        }
                        (None, Some(curr_link)) => {
                            let sub_ctx = NodeContext {
                                bit_width: curr_ctx.bit_width,
                                height: curr_ctx.height - 1,
                                store: curr_ctx.store,
                            };
                            let sub_node = get_sub_node(curr_link, &sub_ctx, curr_ctx.bit_width)?;
                            let new_offset = offset + sub_count * i as u64;
                            changes.append(&mut add_all(&sub_ctx, sub_node.borrow(), new_offset)?);
                        }
                        (Some(prev_link), Some(curr_link)) => {
                            let (prev_cid, prev_sub_node) = match prev_link {
                                node::Link::Cid { cid, cache } => (
                                    Some(cid),
                                    match cache.get() {
                                        Some(n) => Either::Borrowed(n.borrow()),
                                        None => Either::Owned(
                                            prev_ctx
                                                .store
                                                .get_cbor::<CollapsedNode<_>>(cid)?
                                                .context(
                                                    "Failed to get collapsed node from block store",
                                                )?
                                                .expand(prev_ctx.bit_width)?,
                                        ),
                                    },
                                ),
                                node::Link::Dirty(n) => (None, Either::Borrowed(n.borrow())),
                            };
                            let (curr_cid, curr_sub_node) = match curr_link {
                                node::Link::Cid { cid, cache } => (
                                    Some(cid),
                                    match cache.get() {
                                        Some(n) => Either::Borrowed(n.borrow()),
                                        None => Either::Owned(
                                            curr_ctx
                                                .store
                                                .get_cbor::<CollapsedNode<_>>(cid)?
                                                .context(
                                                    "Failed to get collapsed node from block store",
                                                )?
                                                .expand(curr_ctx.bit_width)?,
                                        ),
                                    },
                                ),
                                node::Link::Dirty(n) => (None, Either::Borrowed(n.borrow())),
                            };

                            if let Some(prev_cid) = &prev_cid {
                                if let Some(curr_cid) = &curr_cid {
                                    if prev_cid == curr_cid {
                                        continue;
                                    }
                                }
                            }

                            let prev_sub_ctx = NodeContext {
                                bit_width: prev_ctx.bit_width,
                                height: prev_ctx.height - 1,
                                store: prev_ctx.store,
                            };
                            let curr_sub_ctx = NodeContext {
                                bit_width: curr_ctx.bit_width,
                                height: curr_ctx.height - 1,
                                store: curr_ctx.store,
                            };
                            let new_offset = offset + sub_count * i as u64;

                            changes.append(&mut diff_node(
                                &prev_sub_ctx,
                                prev_sub_node.borrow(),
                                &curr_sub_ctx,
                                curr_sub_node.borrow(),
                                new_offset,
                            )?);
                        }
                    };
                }

                Ok(changes)
            }
            _ => {
                anyhow::bail!("Nodes has no links");
            }
        }
    }
}

fn get_sub_node<'a, V, BS>(
    link: &'a Link<V>,
    sub_ctx: &NodeContext<BS>,
    bit_width: u32,
) -> anyhow::Result<Either<'a, Node<V>>>
where
    V: DeserializeOwned,
    BS: Blockstore,
{
    Ok(match link {
        node::Link::Cid { cid, cache } => {
            if let Some(node) = cache.get() {
                Either::Borrowed(node)
            } else {
                let node = sub_ctx
                    .store
                    .get_cbor::<CollapsedNode<V>>(cid)?
                    .context("Failed to get collapsed node from block store")?
                    .expand(bit_width)?;
                Either::Owned(node)
            }
        }
        node::Link::Dirty(node) => Either::Borrowed(node),
    })
}

enum Either<'a, B: Sized + 'a> {
    Borrowed(&'a B),
    Owned(B),
}

impl<'a, B: Sized + 'a> Borrow<B> for Either<'a, B> {
    fn borrow(&self) -> &B {
        match self {
            Either::Borrowed(b) => b,
            Either::Owned(b) => b,
        }
    }
}
