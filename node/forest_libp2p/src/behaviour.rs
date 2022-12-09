// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    chain_exchange::{ChainExchangeCodec, ChainExchangeProtocolName},
    gossip_params::{build_peer_score_params, build_peer_score_threshold},
};
use crate::{config::Libp2pConfig, discovery::DiscoveryBehaviour};
use crate::{
    discovery::DiscoveryConfig,
    hello::{HelloCodec, HelloProtocolName},
};
use cid::Cid;
use forest_encoding::blake2b_256;
use libipld::store::StoreParams;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{core::identity::Keypair, kad::QueryId};
use libp2p::{core::PeerId, gossipsub::GossipsubMessage};
use libp2p::{
    gossipsub::{
        error::PublishError, error::SubscriptionError, Gossipsub, GossipsubConfigBuilder,
        IdentTopic as Topic, MessageAuthenticity, MessageId, ValidationMode,
    },
    Multiaddr,
};
use libp2p::{identify, ping};
use libp2p::{
    metrics::{Metrics, Recorder},
    request_response::{ProtocolSupport, RequestResponse, RequestResponseConfig},
};
use libp2p_bitswap::{Bitswap, BitswapConfig, BitswapStore};
use log::{debug, warn};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Libp2p behavior for the Forest node. This handles all sub protocols needed for a Filecoin node.
#[derive(NetworkBehaviour)]
pub(crate) struct ForestBehaviour<P: StoreParams> {
    gossipsub: Gossipsub,
    discovery: DiscoveryBehaviour,
    ping: ping::Behaviour,
    identify: identify::Behaviour,
    pub(super) hello: RequestResponse<HelloCodec>,
    pub(super) chain_exchange: RequestResponse<ChainExchangeCodec>,
    pub(super) bitswap: Bitswap<P>,
}

impl<P> Recorder<ForestBehaviourEvent<P>> for Metrics
where
    P: StoreParams,
{
    fn record(&self, event: &ForestBehaviourEvent<P>) {
        match event {
            ForestBehaviourEvent::Gossipsub(e) => self.record(e),
            ForestBehaviourEvent::Ping(ping_event) => self.record(ping_event),
            ForestBehaviourEvent::Identify(id_event) => self.record(id_event),
            _ => {}
        }
    }
}

impl<P: StoreParams> ForestBehaviour<P> {
    pub async fn new<DB: BitswapStore<Params = P>>(
        local_key: &Keypair,
        config: &Libp2pConfig,
        network_name: &str,
        db: DB,
    ) -> Self {
        let mut gs_config_builder = GossipsubConfigBuilder::default();
        gs_config_builder.max_transmit_size(1 << 20);
        gs_config_builder.validation_mode(ValidationMode::Strict);
        gs_config_builder.message_id_fn(|msg: &GossipsubMessage| {
            let s = blake2b_256(&msg.data);
            MessageId::from(s)
        });

        let gossipsub_config = gs_config_builder.build().unwrap();
        let mut gossipsub = Gossipsub::new(
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

        let bitswap = Bitswap::new(BitswapConfig::new(), db);
        if let Err(err) = bitswap.register_metrics(prometheus::default_registry()) {
            warn!("Fail to register prometheus metrics for libp2p_bitswap: {err}");
        }

        let mut discovery_config = DiscoveryConfig::new(local_key.public(), network_name);
        discovery_config
            .with_mdns(config.mdns)
            .with_kademlia(config.kademlia)
            .with_user_defined(config.bootstrap_peers.clone())
            // TODO allow configuring this through config.
            .discovery_limit(config.target_peer_count as u64);

        let hp = std::iter::once((HelloProtocolName, ProtocolSupport::Full));
        let cp = std::iter::once((ChainExchangeProtocolName, ProtocolSupport::Full));

        let mut req_res_config = RequestResponseConfig::default();
        req_res_config.set_request_timeout(Duration::from_secs(20));
        req_res_config.set_connection_keep_alive(Duration::from_secs(20));

        ForestBehaviour {
            gossipsub,
            discovery: discovery_config.finish().await,
            ping: Default::default(),
            identify: identify::Behaviour::new(identify::Config::new(
                "ipfs/0.1.0".into(),
                local_key.public(),
            )),
            bitswap,
            hello: RequestResponse::new(HelloCodec::default(), hp, req_res_config.clone()),
            chain_exchange: RequestResponse::new(ChainExchangeCodec::default(), cp, req_res_config),
        }
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
    pub fn peers(&mut self) -> &HashSet<PeerId> {
        self.discovery.peers()
    }

    /// Returns a map of peer ids and their multi-addresses
    pub fn peer_addresses(&mut self) -> &HashMap<PeerId, Vec<Multiaddr>> {
        self.discovery.peer_addresses()
    }

    /// Send a request for data over bit-swap
    pub fn want_block(&mut self, cid: Cid) -> anyhow::Result<libp2p_bitswap::QueryId> {
        debug!("want {}", cid.to_string());
        let peers = self.discovery.peers().iter().cloned().collect();
        let query_id = self.bitswap.sync(cid, peers, [cid].into_iter());
        Ok(query_id)
    }
}
