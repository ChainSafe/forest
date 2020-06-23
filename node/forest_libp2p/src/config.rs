// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use libp2p::Multiaddr;
use serde::Deserialize;

const DEFAULT_BOOTSTRAP: &[&str] = &[
    "/dns4/bootstrap-0-sin.fil-test.net/tcp/1347/p2p/12D3KooWPdUquftaQvoQEtEdsRBAhwD6jopbF2oweVTzR59VbHEd",
    "/ip4/86.109.15.57/tcp/1347/p2p/12D3KooWPdUquftaQvoQEtEdsRBAhwD6jopbF2oweVTzR59VbHEd",
    "/dns4/bootstrap-0-dfw.fil-test.net/tcp/1347/p2p/12D3KooWQSCkHCzosEyrh8FgYfLejKgEPM5VB6qWzZE3yDAuXn8d",
    "/ip4/139.178.84.45/tcp/1347/p2p/12D3KooWQSCkHCzosEyrh8FgYfLejKgEPM5VB6qWzZE3yDAuXn8d",
    "/dns4/bootstrap-0-fra.fil-test.net/tcp/1347/p2p/12D3KooWEXN2eQmoyqnNjde9PBAQfQLHN67jcEdWU6JougWrgXJK",
    "/ip4/136.144.49.17/tcp/1347/p2p/12D3KooWEXN2eQmoyqnNjde9PBAQfQLHN67jcEdWU6JougWrgXJK",
    "/dns4/bootstrap-1-sin.fil-test.net/tcp/1347/p2p/12D3KooWLmJkZd33mJhjg5RrpJ6NFep9SNLXWc4uVngV4TXKwzYw",
    "/ip4/86.109.15.123/tcp/1347/p2p/12D3KooWLmJkZd33mJhjg5RrpJ6NFep9SNLXWc4uVngV4TXKwzYw",
    "/dns4/bootstrap-1-dfw.fil-test.net/tcp/1347/p2p/12D3KooWGXLHjiz6pTRu7x2pkgTVCoxcCiVxcNLpMnWcJ3JiNEy5",
    "/ip4/139.178.86.3/tcp/1347/p2p/12D3KooWGXLHjiz6pTRu7x2pkgTVCoxcCiVxcNLpMnWcJ3JiNEy5",
    "/dns4/bootstrap-1-fra.fil-test.net/tcp/1347/p2p/12D3KooW9szZmKttS9A1FafH3Zc2pxKwwmvCWCGKkRP4KmbhhC4R",
    "/ip4/136.144.49.131/tcp/1347/p2p/12D3KooW9szZmKttS9A1FafH3Zc2pxKwwmvCWCGKkRP4KmbhhC4R",
];

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Libp2pConfig {
    pub listening_multiaddr: Multiaddr,
    pub bootstrap_peers: Vec<Multiaddr>,
}

impl Default for Libp2pConfig {
    fn default() -> Self {
        let bootstrap_peers = DEFAULT_BOOTSTRAP
            .iter()
            .map(|node| node.parse().unwrap())
            .collect();
        Self {
            listening_multiaddr: "/ip4/0.0.0.0/tcp/0".parse().unwrap(),
            bootstrap_peers,
        }
    }
}
