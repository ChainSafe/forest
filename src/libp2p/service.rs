// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::libp2p_bitswap::{
    request_manager::{BitswapRequestManager, ValidatePeerCallback},
    BitswapStoreRead, BitswapStoreReadWrite,
};
use crate::message::SignedMessage;
use crate::{blocks::GossipBlock, rpc::net::NetInfoResult};
use crate::{chain::ChainStore, utils::encoding::from_slice_with_fallback};
use ahash::{HashMap, HashSet};
use cid::Cid;
use flume::Sender;
use futures::{channel::oneshot, select, stream::StreamExt as _};
use fvm_ipld_blockstore::Blockstore;
pub use libp2p::gossipsub::{IdentTopic, Topic};
use libp2p::{
    autonat::NatStatus,
    connection_limits::Exceeded,
    core::Multiaddr,
    gossipsub,
    identity::Keypair,
    metrics::{Metrics, Recorder},
    multiaddr::Protocol,
    noise, ping, request_response,
    swarm::{DialError, SwarmEvent},
    tcp, yamux, PeerId, Swarm, SwarmBuilder,
};
use tokio_stream::wrappers::IntervalStream;
use tracing::{debug, error, info, trace, warn};

use super::{
    chain_exchange::{make_chain_exchange_response, ChainExchangeRequest, ChainExchangeResponse},
    ForestBehaviour, ForestBehaviourEvent, Libp2pConfig,
};
use crate::libp2p::{
    chain_exchange::ChainExchangeBehaviour,
    discovery::DiscoveryEvent,
    hello::{HelloBehaviour, HelloRequest, HelloResponse},
    rpc::RequestResponseError,
    PeerManager, PeerOperation,
};

pub(in crate::libp2p) mod metrics {
    use once_cell::sync::Lazy;
    use prometheus_client::metrics::{family::Family, gauge::Gauge};

    use crate::metrics::KindLabel;

    pub static NETWORK_CONTAINER_CAPACITIES: Lazy<Family<KindLabel, Gauge>> = {
        Lazy::new(|| {
            let metric = Family::default();
            crate::metrics::default_registry().register(
                "network_container_capacities",
                "Capacity for each container",
                metric.clone(),
            );
            metric
        })
    };

    pub mod values {
        use crate::metrics::KindLabel;

        pub const HELLO_REQUEST_TABLE: KindLabel = KindLabel::new("hello_request_table");
        pub const CHAIN_EXCHANGE_REQUEST_TABLE: KindLabel = KindLabel::new("cx_request_table");
    }
}

/// `Gossipsub` Filecoin blocks topic identifier.
pub const PUBSUB_BLOCK_STR: &str = "/fil/blocks";
/// `Gossipsub` Filecoin messages topic identifier.
pub const PUBSUB_MSG_STR: &str = "/fil/msgs";

const PUBSUB_TOPICS: [&str; 2] = [PUBSUB_BLOCK_STR, PUBSUB_MSG_STR];

pub const BITSWAP_TIMEOUT: Duration = Duration::from_secs(30);

/// Events emitted by this Service.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum NetworkEvent {
    PubsubMessage {
        source: PeerId,
        message: PubsubMessage,
    },
    HelloRequestInbound {
        source: PeerId,
        request: HelloRequest,
    },
    HelloResponseOutbound {
        source: PeerId,
        request: HelloRequest,
    },
    HelloRequestOutbound {
        request_id: request_response::OutboundRequestId,
    },
    HelloResponseInbound {
        request_id: request_response::OutboundRequestId,
    },
    ChainExchangeRequestOutbound {
        request_id: request_response::OutboundRequestId,
    },
    ChainExchangeResponseInbound {
        request_id: request_response::OutboundRequestId,
    },
    ChainExchangeRequestInbound {
        request_id: request_response::InboundRequestId,
    },
    ChainExchangeResponseOutbound {
        request_id: request_response::InboundRequestId,
    },
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
}

/// Message types that can come over `GossipSub`
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum PubsubMessage {
    /// Messages that come over the block topic
    Block(GossipBlock),
    /// Messages that come over the message topic
    Message(SignedMessage),
}

/// Messages into the service to handle.
#[derive(Debug)]
pub enum NetworkMessage {
    PubsubMessage {
        topic: IdentTopic,
        message: Vec<u8>,
    },
    ChainExchangeRequest {
        peer_id: PeerId,
        request: ChainExchangeRequest,
        response_channel: flume::Sender<Result<ChainExchangeResponse, RequestResponseError>>,
    },
    HelloRequest {
        peer_id: PeerId,
        request: HelloRequest,
        response_channel: flume::Sender<HelloResponse>,
    },
    BitswapRequest {
        cid: Cid,
        response_channel: flume::Sender<bool>,
        epoch: Option<i64>,
    },
    JSONRPCRequest {
        method: NetRPCMethods,
    },
}

/// Network RPC API methods used to gather data from libp2p node.
#[derive(Debug)]
pub enum NetRPCMethods {
    AddrsListen(oneshot::Sender<(PeerId, HashSet<Multiaddr>)>),
    Peers(oneshot::Sender<HashMap<PeerId, HashSet<Multiaddr>>>),
    Info(oneshot::Sender<NetInfoResult>),
    Connect(oneshot::Sender<bool>, PeerId, HashSet<Multiaddr>),
    Disconnect(oneshot::Sender<()>, PeerId),
    AgentVersion(oneshot::Sender<Option<String>>, PeerId),
    AutoNATStatus(oneshot::Sender<NatStatus>),
}

/// The `Libp2pService` listens to events from the libp2p swarm.
pub struct Libp2pService<DB> {
    swarm: Swarm<ForestBehaviour>,
    bootstrap_peers: HashMap<PeerId, Multiaddr>,
    cs: Arc<ChainStore<DB>>,
    peer_manager: Arc<PeerManager>,
    network_receiver_in: flume::Receiver<NetworkMessage>,
    network_sender_in: Sender<NetworkMessage>,
    network_receiver_out: flume::Receiver<NetworkEvent>,
    network_sender_out: Sender<NetworkEvent>,
    network_name: String,
    genesis_cid: Cid,
}

impl<DB> Libp2pService<DB>
where
    DB: Blockstore + BitswapStoreReadWrite + Sync + Send + 'static,
{
    pub async fn new(
        config: Libp2pConfig,
        cs: Arc<ChainStore<DB>>,
        peer_manager: Arc<PeerManager>,
        net_keypair: Keypair,
        network_name: &str,
        genesis_cid: Cid,
    ) -> anyhow::Result<Self> {
        let behaviour = ForestBehaviour::new(&net_keypair, &config, network_name)?;
        let mut swarm = SwarmBuilder::with_existing_identity(net_keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default().nodelay(true),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_dns()?
            .with_bandwidth_metrics(&mut crate::metrics::default_registry())
            .with_behaviour(|_| behaviour)?
            .with_swarm_config(|config| {
                config
                    .with_notify_handler_buffer_size(
                        std::num::NonZeroUsize::new(20).expect("Not zero"),
                    )
                    .with_per_connection_event_buffer_size(64)
                    .with_idle_connection_timeout(Duration::from_secs(60 * 10))
            })
            .build();

        // Subscribe to gossipsub topics with the network name suffix
        for topic in PUBSUB_TOPICS.iter() {
            let t = Topic::new(format!("{topic}/{network_name}"));
            swarm.behaviour_mut().subscribe(&t).unwrap();
        }

        let (network_sender_in, network_receiver_in) = flume::unbounded();
        let (network_sender_out, network_receiver_out) = flume::unbounded();

        // Hint at the multihash which has to go in the `/p2p/<multihash>` part of the
        // peer's multiaddress. Useful if others want to use this node to bootstrap
        // from.
        info!("p2p network peer id: {}", swarm.local_peer_id());

        // Listen on network endpoints before being detached and connecting to any peers.
        for addr in &config.listening_multiaddrs {
            match swarm.listen_on(addr.clone()) {
                Ok(id) => loop {
                    if let SwarmEvent::NewListenAddr {
                        address,
                        listener_id,
                    } = swarm.select_next_some().await
                    {
                        if id == listener_id {
                            info!("p2p peer is now listening on: {address}");
                            break;
                        }
                    }
                },
                Err(err) => error!("Fail to listen on {addr}: {err}"),
            }
        }

        if swarm.listeners().count() == 0 {
            anyhow::bail!("p2p peer failed to listen on any network endpoints");
        }

        let bootstrap_peers = config
            .bootstrap_peers
            .iter()
            .filter_map(|ma| match ma.iter().last() {
                Some(Protocol::P2p(peer)) => Some((peer, ma.clone())),
                _ => None,
            })
            .collect();

        Ok(Libp2pService {
            swarm,
            bootstrap_peers,
            cs,
            peer_manager,
            network_receiver_in,
            network_sender_in,
            network_receiver_out,
            network_sender_out,
            network_name: network_name.into(),
            genesis_cid,
        })
    }

    /// Starts the libp2p service networking stack. This Future resolves when
    /// shutdown occurs.
    pub async fn run(mut self) -> anyhow::Result<()> {
        info!("Running libp2p service");

        // Bootstrap with Kademlia
        if let Err(e) = self.swarm.behaviour_mut().bootstrap() {
            warn!("Failed to bootstrap with Kademlia: {e}");
        }

        let bitswap_request_manager = self.swarm.behaviour().bitswap.request_manager();
        let mut swarm_stream = self.swarm.fuse();
        let mut network_stream = self.network_receiver_in.stream().fuse();
        let mut interval =
            IntervalStream::new(tokio::time::interval(Duration::from_secs(15))).fuse();
        let pubsub_block_str = format!("{}/{}", PUBSUB_BLOCK_STR, self.network_name);
        let pubsub_msg_str = format!("{}/{}", PUBSUB_MSG_STR, self.network_name);

        let (cx_response_tx, cx_response_rx) = flume::unbounded();

        let mut cx_response_rx_stream = cx_response_rx.stream().fuse();
        let mut bitswap_outbound_request_stream =
            bitswap_request_manager.outbound_request_stream().fuse();
        let mut peer_ops_rx_stream = self.peer_manager.peer_ops_rx().stream().fuse();
        let metrics = Metrics::new(&mut crate::metrics::default_registry());

        const BOOTSTRAP_PEER_DIALER_INTERVAL: tokio::time::Duration =
            tokio::time::Duration::from_secs(60);
        let mut bootstrap_peer_dialer_interval_stream =
            IntervalStream::new(tokio::time::interval_at(
                tokio::time::Instant::now() + BOOTSTRAP_PEER_DIALER_INTERVAL,
                BOOTSTRAP_PEER_DIALER_INTERVAL,
            ))
            .fuse();
        loop {
            select! {
                swarm_event = swarm_stream.next() => match swarm_event {
                    // outbound events
                    Some(SwarmEvent::Behaviour(event)) => {
                        metrics.record(&event);
                        handle_forest_behaviour_event(
                            swarm_stream.get_mut(),
                            &bitswap_request_manager,
                            &self.peer_manager,
                            event,
                            &self.cs,
                            &self.genesis_cid,
                            &self.network_sender_out,
                            cx_response_tx.clone(),
                            &pubsub_block_str,
                            &pubsub_msg_str,).await;
                    },
                    None => { break; },
                    _ => { },
                },
                rpc_message = network_stream.next() => match rpc_message {
                    // Inbound messages
                    Some(message) => {
                        handle_network_message(
                            swarm_stream.get_mut(),
                            self.cs.clone(),
                            bitswap_request_manager.clone(),
                            message,
                            &self.network_sender_out,
                            &self.peer_manager).await;
                    }
                    None => { break; }
                },
                interval_event = interval.next() => if interval_event.is_some() {
                    // Print peer count on an interval.
                    debug!("Peers connected: {}", swarm_stream.get_mut().behaviour_mut().peers().len());
                },
                cs_pair_opt = cx_response_rx_stream.next() => {
                    if let Some((_request_id, channel, cx_response)) = cs_pair_opt {
                        let behaviour = swarm_stream.get_mut().behaviour_mut();
                        if let Err(e) = behaviour.chain_exchange.send_response(channel, cx_response) {
                            warn!("Error sending chain exchange response: {e:?}");
                        }
                    }
                },
                bitswap_outbound_request_opt = bitswap_outbound_request_stream.next() => {
                    if let Some((peer, request)) = bitswap_outbound_request_opt {
                        let bitswap = &mut swarm_stream.get_mut().behaviour_mut().bitswap;
                        bitswap.send_request(&peer, request);
                    }
                }
                peer_ops_opt = peer_ops_rx_stream.next() => {
                    if let Some(peer_ops) = peer_ops_opt {
                        handle_peer_ops(swarm_stream.get_mut(), peer_ops, &self.bootstrap_peers);
                    }
                },
                _ = bootstrap_peer_dialer_interval_stream.next() => {
                    dial_to_bootstrap_peers_if_needed(swarm_stream.get_mut(), &self.bootstrap_peers);
                }
            };
        }
        Ok(())
    }

    /// Returns a sender which allows sending messages to the libp2p service.
    pub fn network_sender(&self) -> Sender<NetworkMessage> {
        self.network_sender_in.clone()
    }

    /// Returns a receiver to listen to network events emitted from the service.
    pub fn network_receiver(&self) -> flume::Receiver<NetworkEvent> {
        self.network_receiver_out.clone()
    }
}

fn dial_to_bootstrap_peers_if_needed(
    swarm: &mut Swarm<ForestBehaviour>,
    bootstrap_peers: &HashMap<PeerId, Multiaddr>,
) {
    for (peer, ma) in bootstrap_peers {
        if !swarm.behaviour().peers().contains(peer) {
            info!("Re-dialing to bootstrap peer at {ma}");
            if let Err(e) = swarm.dial(ma.clone()) {
                warn!("{e}");
            }
        }
    }
}

fn handle_peer_ops(
    swarm: &mut Swarm<ForestBehaviour>,
    peer_ops: PeerOperation,
    bootstrap_peers: &HashMap<PeerId, Multiaddr>,
) {
    use PeerOperation::*;
    match peer_ops {
        Ban(peer, reason) => {
            // Do not ban bootstrap nodes
            if !bootstrap_peers.contains_key(&peer) {
                warn!(%peer, %reason, "Banning peer");
                swarm.behaviour_mut().blocked_peers.block_peer(peer);
            }
        }
        Unban(peer) => {
            info!(%peer, "Unbanning peer");
            swarm.behaviour_mut().blocked_peers.unblock_peer(peer);
        }
    }
}

async fn handle_network_message(
    swarm: &mut Swarm<ForestBehaviour>,
    store: Arc<impl BitswapStoreReadWrite>,
    bitswap_request_manager: Arc<BitswapRequestManager>,
    message: NetworkMessage,
    network_sender_out: &Sender<NetworkEvent>,
    peer_manager: &Arc<PeerManager>,
) {
    match message {
        NetworkMessage::PubsubMessage { topic, message } => {
            if let Err(e) = swarm.behaviour_mut().publish(topic, message) {
                warn!("Failed to send gossipsub message: {:?}", e);
            }
        }
        NetworkMessage::HelloRequest {
            peer_id,
            request,
            response_channel,
        } => {
            let request_id =
                swarm
                    .behaviour_mut()
                    .hello
                    .send_request(&peer_id, request, response_channel);
            emit_event(
                network_sender_out,
                NetworkEvent::HelloRequestOutbound { request_id },
            )
            .await;
        }
        NetworkMessage::ChainExchangeRequest {
            peer_id,
            request,
            response_channel,
        } => {
            let request_id = swarm.behaviour_mut().chain_exchange.send_request(
                &peer_id,
                request,
                response_channel,
            );
            emit_event(
                network_sender_out,
                NetworkEvent::ChainExchangeRequestOutbound { request_id },
            )
            .await;
        }
        NetworkMessage::BitswapRequest {
            cid,
            response_channel,
            epoch,
        } => {
            let peer_validator: Option<Arc<ValidatePeerCallback>> = if let Some(epoch) = epoch {
                let peer_manager = Arc::clone(peer_manager);
                Some(Arc::new(move |peer| {
                    peer_manager
                        .get_peer_head_epoch(&peer)
                        .map(|peer_head_epoch| peer_head_epoch >= epoch)
                        .unwrap_or_default()
                }))
            } else {
                None
            };

            bitswap_request_manager.get_block(
                store,
                cid,
                BITSWAP_TIMEOUT,
                Some(response_channel),
                peer_validator,
            );
        }
        NetworkMessage::JSONRPCRequest { method } => {
            match method {
                NetRPCMethods::AddrsListen(response_channel) => {
                    let listeners = Swarm::listeners(swarm).cloned().collect();
                    let peer_id = Swarm::local_peer_id(swarm);

                    if response_channel.send((*peer_id, listeners)).is_err() {
                        warn!("Failed to get Libp2p listeners");
                    }
                }
                NetRPCMethods::Peers(response_channel) => {
                    let peer_addresses = swarm.behaviour().peer_addresses();
                    if response_channel.send(peer_addresses).is_err() {
                        warn!("Failed to get Libp2p peers");
                    }
                }
                NetRPCMethods::Info(response_channel) => {
                    if response_channel.send(swarm.network_info().into()).is_err() {
                        warn!("Failed to get Libp2p peers");
                    }
                }
                NetRPCMethods::Connect(response_channel, peer_id, addresses) => {
                    let mut success = false;

                    for mut multiaddr in addresses {
                        multiaddr.push(Protocol::P2p(peer_id));

                        match Swarm::dial(swarm, multiaddr.clone()) {
                            Ok(_) => {
                                info!("Dialed {multiaddr}");
                                success = true;
                                break;
                            }
                            Err(e) => {
                                match e {
                                    DialError::Denied { cause } => {
                                        // try to get a more specific error cause
                                        if let Some(cause) = cause.downcast_ref::<Exceeded>() {
                                            error!("Denied dialing (limits exceeded) {multiaddr}: {cause}");
                                        } else {
                                            error!("Denied dialing {multiaddr}: {cause}")
                                        }
                                    }
                                    e => {
                                        error!("Failed to dial {multiaddr}: {e}");
                                    }
                                };
                            }
                        };
                    }

                    if response_channel.send(success).is_err() {
                        warn!("Failed to connect to a peer");
                    }
                }
                NetRPCMethods::Disconnect(response_channel, peer_id) => {
                    let _ = Swarm::disconnect_peer_id(swarm, peer_id);
                    if response_channel.send(()).is_err() {
                        warn!("Failed to disconnect from a peer");
                    }
                }
                NetRPCMethods::AgentVersion(response_channel, peer_id) => {
                    let agent_version = swarm
                        .behaviour()
                        .peer_info(&peer_id)
                        .and_then(|info| info.agent_version.clone());

                    if response_channel.send(agent_version).is_err() {
                        warn!("Failed to get agent version");
                    }
                }
                NetRPCMethods::AutoNATStatus(response_channel) => {
                    let nat_status = swarm.behaviour().discovery.nat_status();
                    if response_channel.send(nat_status).is_err() {
                        warn!("Failed to get nat status");
                    }
                }
            }
        }
    }
}

async fn handle_discovery_event(
    discovery_out: DiscoveryEvent,
    network_sender_out: &Sender<NetworkEvent>,
) {
    match discovery_out {
        DiscoveryEvent::PeerConnected(peer_id) => {
            debug!("Peer connected, {:?}", peer_id);
            emit_event(network_sender_out, NetworkEvent::PeerConnected(peer_id)).await;
        }
        DiscoveryEvent::PeerDisconnected(peer_id) => {
            debug!("Peer disconnected, {:?}", peer_id);
            emit_event(network_sender_out, NetworkEvent::PeerDisconnected(peer_id)).await;
        }
        DiscoveryEvent::Discovery(_) => {}
    }
}

async fn handle_gossip_event(
    e: gossipsub::Event,
    network_sender_out: &Sender<NetworkEvent>,
    pubsub_block_str: &str,
    pubsub_msg_str: &str,
) {
    if let gossipsub::Event::Message {
        propagation_source: source,
        message,
        message_id: _,
    } = e
    {
        let topic = message.topic.as_str();
        let message = message.data;
        trace!("Got a Gossip Message from {:?}", source);
        if topic == pubsub_block_str {
            match from_slice_with_fallback::<GossipBlock>(&message) {
                Ok(b) => {
                    emit_event(
                        network_sender_out,
                        NetworkEvent::PubsubMessage {
                            source,
                            message: PubsubMessage::Block(b),
                        },
                    )
                    .await;
                }
                Err(e) => {
                    warn!("Gossip Block from peer {source:?} could not be deserialized: {e}",);
                }
            }
        } else if topic == pubsub_msg_str {
            match from_slice_with_fallback::<SignedMessage>(&message) {
                Ok(m) => {
                    emit_event(
                        network_sender_out,
                        NetworkEvent::PubsubMessage {
                            source,
                            message: PubsubMessage::Message(m),
                        },
                    )
                    .await;
                }
                Err(e) => {
                    warn!("Gossip Message from peer {source:?} could not be deserialized: {e}");
                }
            }
        } else {
            warn!("Getting gossip messages from unknown topic: {topic}");
        }
    }
}

async fn handle_hello_event(
    hello: &mut HelloBehaviour,
    event: request_response::Event<HelloRequest, HelloResponse, HelloResponse>,
    peer_manager: &Arc<PeerManager>,
    genesis_cid: &Cid,
    network_sender_out: &Sender<NetworkEvent>,
) {
    match event {
        request_response::Event::Message { peer, message } => match message {
            request_response::Message::Request {
                request,
                channel,
                request_id: _,
            } => {
                emit_event(
                    network_sender_out,
                    NetworkEvent::HelloRequestInbound {
                        source: peer,
                        request: request.clone(),
                    },
                )
                .await;

                let arrival = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("System time before unix epoch")
                    .as_nanos()
                    .try_into()
                    .expect("System time since unix epoch should not exceed u64");

                trace!("Received hello request: {:?}", request);
                if &request.genesis_cid != genesis_cid {
                    peer_manager
                        .ban_peer_with_default_duration(
                            peer,
                            format!(
                                "Genesis hash mismatch: {} received, {genesis_cid} expected",
                                request.genesis_cid
                            ),
                        )
                        .await;
                } else {
                    let sent = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("System time before unix epoch")
                        .as_nanos()
                        .try_into()
                        .expect("System time since unix epoch should not exceed u64");

                    // Send hello response immediately, no need to have the overhead of emitting
                    // channel and polling future here.
                    if let Err(e) = hello.send_response(channel, HelloResponse { arrival, sent }) {
                        warn!("Failed to send HelloResponse: {e:?}");
                    } else {
                        emit_event(
                            network_sender_out,
                            NetworkEvent::HelloResponseOutbound {
                                source: peer,
                                request,
                            },
                        )
                        .await;
                    }
                }
            }
            request_response::Message::Response {
                request_id,
                response,
            } => {
                emit_event(
                    network_sender_out,
                    NetworkEvent::HelloResponseInbound { request_id },
                )
                .await;
                hello.handle_response(&request_id, response).await;
            }
        },
        request_response::Event::OutboundFailure {
            request_id,
            peer,
            error,
        } => {
            hello.on_outbound_failure(&request_id);
            match error {
                request_response::OutboundFailure::UnsupportedProtocols => {
                    peer_manager
                        .ban_peer_with_default_duration(peer, "Hello protocol unsupported")
                        .await;
                }
                _ => {
                    peer_manager.mark_peer_bad(peer, format!("Hello outbound failure {error}"));
                }
            }
        }
        request_response::Event::InboundFailure { .. } => {}
        request_response::Event::ResponseSent { .. } => (),
    }
}

async fn handle_ping_event(ping_event: ping::Event, peer_manager: &Arc<PeerManager>) {
    match ping_event.result {
        Ok(rtt) => {
            trace!(
                "PingSuccess::Ping rtt to {} is {} ms",
                ping_event.peer.to_base58(),
                rtt.as_millis()
            );
        }
        Err(ping::Failure::Unsupported) => {
            peer_manager
                .ban_peer_with_default_duration(ping_event.peer, "Ping protocol unsupported")
                .await;
        }
        Err(ping::Failure::Timeout) => {
            warn!("Ping timeout: {}", ping_event.peer);
        }
        Err(ping::Failure::Other { error }) => {
            debug!("Ping failure: {error}");
        }
    }
}

async fn handle_chain_exchange_event<DB>(
    chain_exchange: &mut ChainExchangeBehaviour,
    ce_event: request_response::Event<ChainExchangeRequest, ChainExchangeResponse>,
    db: &Arc<ChainStore<DB>>,
    network_sender_out: &Sender<NetworkEvent>,
    cx_response_tx: Sender<(
        request_response::InboundRequestId,
        request_response::ResponseChannel<ChainExchangeResponse>,
        ChainExchangeResponse,
    )>,
) where
    DB: Blockstore + Sync + Send + 'static,
{
    match ce_event {
        request_response::Event::Message { peer, message } => match message {
            request_response::Message::Request {
                request,
                channel,
                request_id,
            } => {
                trace!(
                    "Received chain_exchange request (request_id:{request_id}, peer_id: {peer:?})",
                );
                emit_event(
                    network_sender_out,
                    NetworkEvent::ChainExchangeRequestInbound { request_id },
                )
                .await;

                let db = db.clone();
                tokio::task::spawn(async move {
                    if let Err(e) = cx_response_tx.send((
                        request_id,
                        channel,
                        make_chain_exchange_response(&db, &request),
                    )) {
                        debug!("Failed to send ChainExchangeResponse: {e:?}");
                    }
                });
            }
            request_response::Message::Response {
                request_id,
                response,
            } => {
                emit_event(
                    network_sender_out,
                    NetworkEvent::ChainExchangeResponseInbound { request_id },
                )
                .await;
                chain_exchange
                    .handle_inbound_response(&request_id, response)
                    .await;
            }
        },
        request_response::Event::OutboundFailure {
            peer: _,
            request_id,
            error,
        } => {
            chain_exchange.on_outbound_error(&request_id, error);
        }
        request_response::Event::InboundFailure {
            peer,
            error,
            request_id: _,
        } => {
            debug!(
                "ChainExchange inbound error (peer: {:?}): {:?}",
                peer, error
            );
        }
        request_response::Event::ResponseSent { request_id, .. } => {
            emit_event(
                network_sender_out,
                NetworkEvent::ChainExchangeResponseOutbound { request_id },
            )
            .await;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_forest_behaviour_event<DB>(
    swarm: &mut Swarm<ForestBehaviour>,
    bitswap_request_manager: &Arc<BitswapRequestManager>,
    peer_manager: &Arc<PeerManager>,
    event: ForestBehaviourEvent,
    db: &Arc<ChainStore<DB>>,
    genesis_cid: &Cid,
    network_sender_out: &Sender<NetworkEvent>,
    cx_response_tx: Sender<(
        request_response::InboundRequestId,
        request_response::ResponseChannel<ChainExchangeResponse>,
        ChainExchangeResponse,
    )>,
    pubsub_block_str: &str,
    pubsub_msg_str: &str,
) where
    DB: Blockstore + BitswapStoreRead + Sync + Send + 'static,
{
    match event {
        ForestBehaviourEvent::Discovery(discovery_out) => {
            handle_discovery_event(discovery_out, network_sender_out).await
        }
        ForestBehaviourEvent::Gossipsub(e) => {
            handle_gossip_event(e, network_sender_out, pubsub_block_str, pubsub_msg_str).await
        }
        ForestBehaviourEvent::Hello(rr_event) => {
            handle_hello_event(
                &mut swarm.behaviour_mut().hello,
                rr_event,
                peer_manager,
                genesis_cid,
                network_sender_out,
            )
            .await
        }
        ForestBehaviourEvent::Bitswap(event) => {
            if let Err(e) = bitswap_request_manager.handle_event(
                &mut swarm.behaviour_mut().bitswap,
                db.blockstore(),
                event,
            ) {
                warn!("bitswap: {e}");
            }
        }
        ForestBehaviourEvent::Ping(ping_event) => handle_ping_event(ping_event, peer_manager).await,
        ForestBehaviourEvent::ConnectionLimits(_) => {}
        ForestBehaviourEvent::BlockedPeers(_) => {}
        ForestBehaviourEvent::ChainExchange(ce_event) => {
            handle_chain_exchange_event(
                &mut swarm.behaviour_mut().chain_exchange,
                ce_event,
                db,
                network_sender_out,
                cx_response_tx,
            )
            .await
        }
    }
}

async fn emit_event(sender: &Sender<NetworkEvent>, event: NetworkEvent) {
    if sender.send_async(event).await.is_err() {
        error!("Failed to emit event: Network channel receiver has been dropped");
    }
}
