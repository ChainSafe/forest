// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use crate::libp2p_bitswap::BitswapBehaviour;
use crate::utils::{encoding::blake2b_256, version::FOREST_VERSION_STRING};
use ahash::{HashMap, HashSet};
use libp2p::{
    allow_block_list, connection_limits,
    gossipsub::{
        self, IdentTopic as Topic, MessageAuthenticity, MessageId, PublishError, SubscriptionError,
        ValidationMode,
    },
    identity::{Keypair, PeerId},
    kad::QueryId,
    metrics::{Metrics, Recorder},
    ping,
    swarm::NetworkBehaviour,
    Multiaddr,
};
use tracing::info;

use crate::libp2p::{
    chain_exchange::ChainExchangeBehaviour,
    config::Libp2pConfig,
    discovery::{DiscoveryBehaviour, DiscoveryConfig},
    gossip_params::{build_peer_score_params, build_peer_score_threshold},
    hello::HelloBehaviour,
};

use super::discovery::{DerivedDiscoveryBehaviourEvent, DiscoveryEvent, PeerInfo};

/// Libp2p behavior for the Forest node. This handles all sub protocols needed
/// for a Filecoin node.
#[derive(NetworkBehaviour)]
pub(in crate::libp2p) struct ForestBehaviour {
    // Behaviours that manage connections should come first, to get rid of some panics in debug build.
    // See <https://github.com/libp2p/rust-libp2p/issues/4773#issuecomment-2042676966>
    connection_limits: connection_limits::Behaviour,
    pub(super) blocked_peers: allow_block_list::Behaviour<allow_block_list::BlockedPeers>,
    pub(super) discovery: DiscoveryBehaviour,
    ping: ping::Behaviour,
    gossipsub: gossipsub::Behaviour,
    pub(super) hello: HelloBehaviour,
    pub(super) chain_exchange: ChainExchangeBehaviour,
    pub(super) bitswap: BitswapBehaviour,
}

impl Recorder<ForestBehaviourEvent> for Metrics {
    fn record(&self, event: &ForestBehaviourEvent) {
        match event {
            ForestBehaviourEvent::Gossipsub(e) => self.record(e),
            ForestBehaviourEvent::Ping(ping_event) => self.record(ping_event),
            ForestBehaviourEvent::Discovery(DiscoveryEvent::Discovery(e)) => match e.as_ref() {
                DerivedDiscoveryBehaviourEvent::Identify(e) => self.record(e),
                DerivedDiscoveryBehaviourEvent::Kademlia(e) => self.record(e),
                _ => {}
            },
            _ => {}
        }
    }
}

impl ForestBehaviour {
    pub fn new(
        local_key: &Keypair,
        config: &Libp2pConfig,
        network_name: &str,
    ) -> anyhow::Result<Self> {
        let gossipsub_config = {
            let mut builder = gossipsub::ConfigBuilder::default();
            builder
                .max_transmit_size(1 << 20)
                .validation_mode(ValidationMode::Strict)
                .message_id_fn(|msg: &gossipsub::Message| blake2b_256(&msg.data).into());

            // https://github.com/filecoin-project/lotus/blob/v1.27.0/node/modules/lp2p/pubsub.go#L27
            builder
                .mesh_n(8)
                .retain_scores(6)
                .mesh_outbound_min(3)
                .mesh_n_low(6)
                .mesh_n_high(12)
                .gossip_lazy(12)
                .heartbeat_initial_delay(Duration::from_secs(30))
                .heartbeat_interval(Duration::from_secs(5))
                .history_length(10)
                .gossip_factor(0.1);

            if config.bootstrap {
                // https://github.com/filecoin-project/lotus/blob/v1.27.0/node/modules/lp2p/pubsub.go#L322
                builder
                    .do_px()
                    .mesh_n(0)
                    .retain_scores(0)
                    .mesh_outbound_min(0)
                    .mesh_n_low(0)
                    .mesh_n_high(0)
                    .gossip_lazy(64)
                    .gossip_factor(0.25)
                    .prune_backoff(Duration::from_secs(5 * 60));
            }

            builder.build()?
        };
        let mut gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )
        .unwrap();

        gossipsub
            .with_peer_score(
                build_peer_score_params(network_name),
                build_peer_score_threshold(),
            )
            .unwrap();

        let bitswap = BitswapBehaviour::new(
            &[
                "/chain/ipfs/bitswap/1.2.0",
                "/chain/ipfs/bitswap/1.1.0",
                "/chain/ipfs/bitswap/1.0.0",
                "/chain/ipfs/bitswap",
            ],
            Default::default(),
        );
        crate::libp2p_bitswap::register_metrics(&mut crate::metrics::default_registry());

        let discovery = DiscoveryConfig::new(local_key.public(), network_name)
            .with_mdns(config.mdns)
            .with_kademlia(config.kademlia)
            .with_user_defined(config.bootstrap_peers.clone())?
            .target_peer_count(config.target_peer_count as u64)
            .finish()?;

        const MAX_ESTABLISHED_PER_PEER: u32 = 4;
        let connection_limits = connection_limits::Behaviour::new(
            connection_limits::ConnectionLimits::default()
                .with_max_pending_incoming(Some(
                    config
                        .target_peer_count
                        .saturating_mul(MAX_ESTABLISHED_PER_PEER),
                ))
                .with_max_pending_outgoing(Some(
                    config
                        .target_peer_count
                        .saturating_mul(MAX_ESTABLISHED_PER_PEER),
                ))
                .with_max_established_incoming(Some(
                    config
                        .target_peer_count
                        .saturating_mul(MAX_ESTABLISHED_PER_PEER),
                ))
                .with_max_established_outgoing(Some(
                    config
                        .target_peer_count
                        .saturating_mul(MAX_ESTABLISHED_PER_PEER),
                ))
                .with_max_established_per_peer(Some(MAX_ESTABLISHED_PER_PEER)),
        );

        info!("libp2p Forest version: {}", FOREST_VERSION_STRING.as_str());
        Ok(ForestBehaviour {
            gossipsub,
            discovery,
            ping: Default::default(),
            connection_limits,
            blocked_peers: Default::default(),
            bitswap,
            hello: HelloBehaviour::default(),
            chain_exchange: ChainExchangeBehaviour::default(),
        })
    }

    /// Bootstrap Kademlia network
    pub fn bootstrap(&mut self) -> Result<QueryId, String> {
        self.discovery.bootstrap()
    }

    /// Publish data over the gossip network.
    pub fn publish(
        &mut self,
        topic: Topic,
        data: impl Into<Vec<u8>>,
    ) -> Result<MessageId, PublishError> {
        self.gossipsub.publish(topic, data)
    }

    /// Subscribe to a gossip topic.
    pub fn subscribe(&mut self, topic: &Topic) -> Result<bool, SubscriptionError> {
        self.gossipsub.subscribe(topic)
    }

    /// Returns a set of peer ids
    pub fn peers(&self) -> &HashSet<PeerId> {
        self.discovery.peers()
    }

    /// Returns a map of peer ids and their multi-addresses
    pub fn peer_addresses(&self) -> HashMap<PeerId, HashSet<Multiaddr>> {
        self.discovery.peer_addresses()
    }

    pub fn peer_info(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.discovery.peer_info(peer_id)
    }
}
