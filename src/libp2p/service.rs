// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::blocks::GossipBlock;
use crate::chain::ChainStore;
use crate::libp2p_bitswap::{
    request_manager::BitswapRequestManager, BitswapStoreRead, BitswapStoreReadWrite,
};
use crate::message::SignedMessage;
use crate::shim::clock::ChainEpoch;
use crate::utils::io::read_file_to_vec;
use ahash::{HashMap, HashSet};
use anyhow::Context;
use cid::Cid;
use flume::Sender;
use futures::{channel::oneshot::Sender as OneShotSender, select};
use futures_util::stream::StreamExt;
use fvm_ipld_blockstore::Blockstore;
pub use libp2p::gossipsub::{IdentTopic, Topic};
use libp2p::{
    core::{self, muxing::StreamMuxerBox, transport::Boxed, Multiaddr},
    gossipsub,
    identity::Keypair,
    metrics::{Metrics, Recorder},
    multiaddr::Protocol,
    noise, ping,
    request_response::{self, RequestId, ResponseChannel},
    swarm::{SwarmBuilder, SwarmEvent},
    yamux, PeerId, Swarm, Transport,
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
    use lazy_static::lazy_static;
    use prometheus::core::{AtomicU64, GenericGaugeVec, Opts};
    lazy_static! {
        pub static ref NETWORK_CONTAINER_CAPACITIES: Box<GenericGaugeVec<AtomicU64>> = {
            let network_container_capacities = Box::new(
                GenericGaugeVec::<AtomicU64>::new(
                    Opts::new(
                        "network_container_capacities",
                        "Capacity for each container",
                    ),
                    &[labels::KIND],
                )
                .expect("Defining the network_container_capacities metric must succeed"),
            );
            prometheus::default_registry().register(network_container_capacities.clone()).expect(
                "Registering the network_container_capacities metric with the metrics registry must succeed"
            );
            network_container_capacities
        };
    }

    pub mod values {
        pub const HELLO_REQUEST_TABLE: &str = "hello_request_table";
        pub const CHAIN_EXCHANGE_REQUEST_TABLE: &str = "cx_request_table";
    }

    pub mod labels {
        pub const KIND: &str = "kind";
    }
}

/// `Gossipsub` Filecoin blocks topic identifier.
pub const PUBSUB_BLOCK_STR: &str = "/fil/blocks";
/// `Gossipsub` Filecoin messages topic identifier.
pub const PUBSUB_MSG_STR: &str = "/fil/msgs";

const PUBSUB_TOPICS: [&str; 2] = [PUBSUB_BLOCK_STR, PUBSUB_MSG_STR];

pub const BITSWAP_TIMEOUT: Duration = Duration::from_secs(10);

const BAN_PEER_DURATION: Duration = Duration::from_secs(60 * 60); //1h

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
        request_id: RequestId,
    },
    HelloResponseInbound {
        request_id: RequestId,
    },
    ChainExchangeRequestOutbound {
        request_id: RequestId,
    },
    ChainExchangeResponseInbound {
        request_id: RequestId,
    },
    ChainExchangeRequestInbound {
        request_id: RequestId,
    },
    ChainExchangeResponseOutbound {
        request_id: RequestId,
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
        epoch: ChainEpoch,
        cid: Cid,
        response_channel: flume::Sender<bool>,
    },
    JSONRPCRequest {
        method: NetRPCMethods,
    },
}

/// Network RPC API methods used to gather data from libp2p node.
#[derive(Debug)]
pub enum NetRPCMethods {
    AddrsListen(OneShotSender<(PeerId, HashSet<Multiaddr>)>),
    Peers(OneShotSender<HashMap<PeerId, HashSet<Multiaddr>>>),
    Connect(OneShotSender<bool>, PeerId, HashSet<Multiaddr>),
    Disconnect(OneShotSender<()>, PeerId),
}

/// The `Libp2pService` listens to events from the libp2p swarm.
pub struct Libp2pService<DB> {
    config: Libp2pConfig,
    swarm: Swarm<ForestBehaviour>,
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
    DB: Blockstore + BitswapStoreReadWrite + Clone + Sync + Send + 'static,
{
    pub fn new(
        config: Libp2pConfig,
        cs: Arc<ChainStore<DB>>,
        peer_manager: Arc<PeerManager>,
        net_keypair: Keypair,
        network_name: &str,
        genesis_cid: Cid,
    ) -> anyhow::Result<Self> {
        let peer_id = PeerId::from(net_keypair.public());

        let transport =
            build_transport(net_keypair.clone()).expect("Failed to build libp2p transport");

        let mut swarm = SwarmBuilder::with_tokio_executor(
            transport,
            ForestBehaviour::new(&net_keypair, &config, network_name)?,
            peer_id,
        )
        .notify_handler_buffer_size(std::num::NonZeroUsize::new(20).expect("Not zero"))
        .per_connection_event_buffer_size(64)
        .build();

        // Subscribe to gossipsub topics with the network name suffix
        for topic in PUBSUB_TOPICS.iter() {
            let t = Topic::new(format!("{topic}/{network_name}"));
            swarm.behaviour_mut().subscribe(&t).unwrap();
        }

        let (network_sender_in, network_receiver_in) = flume::unbounded();
        let (network_sender_out, network_receiver_out) = flume::unbounded();

        Ok(Libp2pService {
            config,
            swarm,
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
        for addr in &self.config.listening_multiaddrs {
            if let Err(err) = Swarm::listen_on(&mut self.swarm, addr.clone()) {
                error!("Fail to listen on {addr}: {err}");
            }
        }

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
        let mut bitswap_outbound_request_rx_stream = bitswap_request_manager
            .outbound_request_rx()
            .stream()
            .fuse();
        let mut peer_ops_rx_stream = self.peer_manager.peer_ops_rx().stream().fuse();
        let mut libp2p_registry = Default::default();
        let metrics = Metrics::new(&mut libp2p_registry);
        crate::metrics::add_metrics_registry("libp2p".into(), libp2p_registry).await;
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
                            &self.network_sender_out).await;
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
                bitswap_outbound_request_opt = bitswap_outbound_request_rx_stream.next() => {
                    if let Some((peer, request)) = bitswap_outbound_request_opt {
                        let bitswap = &mut swarm_stream.get_mut().behaviour_mut().bitswap;
                        bitswap.send_request(&peer, request);
                    }
                }
                peer_ops_opt = peer_ops_rx_stream.next() => {
                    if let Some(peer_ops) = peer_ops_opt {
                        handle_peer_ops(swarm_stream.get_mut(), peer_ops);
                    }
                },
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

fn handle_peer_ops(swarm: &mut Swarm<ForestBehaviour>, peer_ops: PeerOperation) {
    use PeerOperation::*;
    match peer_ops {
        Ban(peer_id, reason) => {
            warn!("Banning {peer_id}, reason: {reason}");
            swarm.behaviour_mut().blocked_peers.block_peer(peer_id);
        }
        Unban(peer_id) => {
            info!("Unbanning {peer_id}");
            swarm.behaviour_mut().blocked_peers.unblock_peer(peer_id);
        }
    }
}

async fn handle_network_message(
    swarm: &mut Swarm<ForestBehaviour>,
    store: Arc<impl BitswapStoreReadWrite>,
    bitswap_request_manager: Arc<BitswapRequestManager>,
    message: NetworkMessage,
    network_sender_out: &Sender<NetworkEvent>,
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
            epoch: _,
            cid,
            response_channel,
        } => {
            bitswap_request_manager.get_block(store, cid, BITSWAP_TIMEOUT, Some(response_channel));
        }
        NetworkMessage::JSONRPCRequest { method } => match method {
            NetRPCMethods::AddrsListen(response_channel) => {
                let listeners = Swarm::listeners(swarm).cloned().collect();
                let peer_id = Swarm::local_peer_id(swarm);

                if response_channel.send((*peer_id, listeners)).is_err() {
                    warn!("Failed to get Libp2p listeners");
                }
            }
            NetRPCMethods::Peers(response_channel) => {
                let peer_addresses = swarm.behaviour_mut().peer_addresses();
                if response_channel.send(peer_addresses.clone()).is_err() {
                    warn!("Failed to get Libp2p peers");
                }
            }
            NetRPCMethods::Connect(response_channel, peer_id, addresses) => {
                let mut success = false;

                for mut multiaddr in addresses {
                    multiaddr.push(Protocol::P2p(peer_id));

                    if Swarm::dial(swarm, multiaddr.clone()).is_ok() {
                        success = true;
                        break;
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
        },
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
            match fvm_ipld_encoding::from_slice::<GossipBlock>(&message) {
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
            match fvm_ipld_encoding::from_slice::<SignedMessage>(&message) {
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
                        .ban_peer(
                            peer,
                            format!(
                                "Genesis hash mismatch: {} received, {genesis_cid} expected",
                                request.genesis_cid
                            ),
                            Some(BAN_PEER_DURATION),
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
            error: _,
        } => {
            hello.on_error(&request_id);
            peer_manager.mark_peer_bad(peer).await;
        }
        request_response::Event::InboundFailure {
            request_id,
            peer: _,
            error: _,
        } => {
            hello.on_error(&request_id);
        }
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
                .ban_peer(
                    ping_event.peer,
                    format!("Ping protocol unsupported: {}", ping_event.peer),
                    Some(BAN_PEER_DURATION),
                )
                .await;
        }
        Err(ping::Failure::Timeout) => {
            warn!("Ping timeout: {}", ping_event.peer);
        }
        Err(ping::Failure::Other { error }) => {
            peer_manager
                .ban_peer(
                    ping_event.peer,
                    format!("PingFailure::Other {}: {error}", ping_event.peer),
                    Some(BAN_PEER_DURATION),
                )
                .await;
        }
    }
}

async fn handle_chain_exchange_event<DB>(
    chain_exchange: &mut ChainExchangeBehaviour,
    ce_event: request_response::Event<ChainExchangeRequest, ChainExchangeResponse>,
    db: &Arc<ChainStore<DB>>,
    network_sender_out: &Sender<NetworkEvent>,
    cx_response_tx: Sender<(
        RequestId,
        ResponseChannel<ChainExchangeResponse>,
        ChainExchangeResponse,
    )>,
) where
    DB: Blockstore + Clone + Sync + Send + 'static,
{
    match ce_event {
        request_response::Event::Message { peer, message } => {
            match message {
                request_response::Message::Request {
                    request,
                    channel,
                    request_id,
                } => {
                    trace!("Received chain_exchange request (request_id:{request_id}, peer_id: {peer:?})",);
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
                            make_chain_exchange_response(db.as_ref(), &request),
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
            }
        }
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
        RequestId,
        ResponseChannel<ChainExchangeResponse>,
        ChainExchangeResponse,
    )>,
    pubsub_block_str: &str,
    pubsub_msg_str: &str,
) where
    DB: Blockstore + BitswapStoreRead + Clone + Sync + Send + 'static,
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
        ForestBehaviourEvent::Identify(_) => {}
        ForestBehaviourEvent::KeepAlive(_) => {}
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

/// Builds the transport stack that libp2p will communicate over. When support
/// of other protocols like `udp`, `quic`, `http` are added, remember to update
/// code comment in [`Libp2pConfig`].
///
/// As a reference `lotus` uses the default `go-libp2p` transport builder which
/// has all above protocols enabled.
pub fn build_transport(local_key: Keypair) -> anyhow::Result<Boxed<(PeerId, StreamMuxerBox)>> {
    let build_tcp = || libp2p::tcp::tokio::Transport::new(libp2p::tcp::Config::new().nodelay(true));
    let build_dns_tcp = || libp2p::dns::TokioDnsConfig::system(build_tcp());
    let transport =
        libp2p::websocket::WsConfig::new(build_dns_tcp()?).or_transport(build_dns_tcp()?);

    let auth_config = noise::Config::new(&local_key).context("Noise key generation failed")?;

    Ok(transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(auth_config)
        .multiplex(yamux::Config::default())
        .timeout(Duration::from_secs(20))
        .boxed())
}

/// Fetch key-pair from disk, returning none if it cannot be decoded.
pub fn get_keypair(path: &Path) -> Option<Keypair> {
    match read_file_to_vec(path) {
        Err(e) => {
            info!("Networking keystore not found!");
            trace!("Error {:?}", e);
            None
        }
        Ok(mut vec) => match Keypair::ed25519_from_bytes(&mut vec) {
            Ok(kp) => {
                info!("Recovered libp2p keypair from {:?}", &path);
                Some(kp)
            }
            Err(e) => {
                info!("Could not decode networking keystore!");
                trace!("Error {:?}", e);
                None
            }
        },
    }
}
