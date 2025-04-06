// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{num::NonZeroUsize, sync::Arc};

use super::{
    PeerManager,
    discovery::{DerivedDiscoveryBehaviourEvent, DiscoveryEvent, PeerInfo},
};
use crate::libp2p::{
    chain_exchange::ChainExchangeBehaviour,
    config::Libp2pConfig,
    discovery::{DiscoveryBehaviour, DiscoveryConfig},
    gossip_params::{build_peer_score_params, build_peer_score_threshold},
    hello::HelloBehaviour,
};
use crate::libp2p_bitswap::BitswapBehaviour;
use crate::utils::{encoding::blake2b_256, version::FOREST_VERSION_STRING};
use ahash::{HashMap, HashSet};
use libp2p::{
    Multiaddr, allow_block_list, connection_limits,
    gossipsub::{
        self, IdentTopic as Topic, MessageAuthenticity, MessageId, PublishError, SubscriptionError,
        ValidationMode,
    },
    identity::{Keypair, PeerId},
    kad::QueryId,
    metrics::{Metrics, Recorder},
    ping, request_response,
    swarm::NetworkBehaviour,
};
use once_cell::sync::Lazy;
use tracing::info;

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
    pub async fn new(
        local_key: &Keypair,
        config: &Libp2pConfig,
        network_name: &str,
        peer_manager: Arc<PeerManager>,
    ) -> anyhow::Result<Self> {
        const MAX_ESTABLISHED_PER_PEER: u32 = 4;
        static MAX_CONCURRENT_REQUEST_RESPONSE_STREAMS_PER_PEER: Lazy<usize> = Lazy::new(|| {
            std::env::var("FOREST_MAX_CONCURRENT_REQUEST_RESPONSE_STREAMS_PER_PEER")
                .ok()
                .map(|it|
                    it.parse::<NonZeroUsize>()
                        .expect("Failed to parse the `FOREST_MAX_CONCURRENT_REQUEST_RESPONSE_STREAMS_PER_PEER` environment variable value, a positive integer is expected.")
                        .get())
                .unwrap_or(10)
        });

        let max_concurrent_request_response_streams = (config.target_peer_count as usize)
            .saturating_mul(*MAX_CONCURRENT_REQUEST_RESPONSE_STREAMS_PER_PEER);

        let mut gs_config_builder = gossipsub::ConfigBuilder::default();
        gs_config_builder.max_transmit_size(1 << 20);
        gs_config_builder.validation_mode(ValidationMode::Strict);
        gs_config_builder.message_id_fn(|msg: &gossipsub::Message| {
            let s = blake2b_256(&msg.data);
            MessageId::from(s)
        });

        let gossipsub_config = gs_config_builder.build().unwrap();
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
            request_response::Config::default()
                .with_max_concurrent_streams(max_concurrent_request_response_streams),
        );
        crate::libp2p_bitswap::register_metrics(&mut crate::metrics::default_registry());

        let discovery = DiscoveryConfig::new(local_key.public(), network_name)
            .with_mdns(config.mdns)
            .with_kademlia(config.kademlia)
            .with_user_defined(config.bootstrap_peers.clone())
            .await?
            .target_peer_count(config.target_peer_count as u64)
            .finish()?;

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
            hello: HelloBehaviour::new(
                request_response::Config::default()
                    .with_max_concurrent_streams(max_concurrent_request_response_streams),
                peer_manager,
            ),
            chain_exchange: ChainExchangeBehaviour::new(
                request_response::Config::default()
                    .with_max_concurrent_streams(max_concurrent_request_response_streams),
            ),
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
