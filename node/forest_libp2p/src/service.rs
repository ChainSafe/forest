// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::chain_exchange::{
    make_chain_exchange_response, ChainExchangeRequest, ChainExchangeResponse,
};
use super::{ForestBehaviour, ForestBehaviourEvent, Libp2pConfig};
use crate::discovery::DiscoveryOut;
use crate::{
    hello::{HelloRequest, HelloResponse},
    rpc::RequestResponseError,
};
use crate::{PeerManager, PeerOperation};
use cid::Cid;
use flume::Sender;
use forest_blocks::GossipBlock;
use forest_chain::ChainStore;
use forest_db::Store;
use forest_message::SignedMessage;
use forest_utils::io::read_file_to_vec;
use futures::channel::oneshot::Sender as OneShotSender;
use futures::select;
use futures_util::stream::StreamExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::from_slice;
use libipld::store::StoreParams;
use libp2p::gossipsub::GossipsubEvent;
pub use libp2p::gossipsub::IdentTopic;
pub use libp2p::gossipsub::Topic;
use libp2p::metrics::{Metrics, Recorder};
use libp2p::multiaddr::Protocol;
use libp2p::multihash::Multihash;
use libp2p::ping::{self};
use libp2p::request_response::{
    RequestId, RequestResponseEvent, RequestResponseMessage, ResponseChannel,
};
use libp2p::{
    core,
    core::muxing::StreamMuxerBox,
    core::transport::Boxed,
    identity::{ed25519, Keypair},
    mplex, noise,
    swarm::{ConnectionLimits, SwarmEvent},
    yamux, PeerId, Swarm, Transport,
};
use libp2p::{core::Multiaddr, swarm::SwarmBuilder};
use libp2p_bitswap::{BitswapEvent, BitswapStore};
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_stream::wrappers::IntervalStream;

mod metrics {
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
        pub const CX_REQUEST_TABLE: &str = "cx_request_table";
        pub const BITSWAP_OUTGOING_QUERY_IDS: &str = "bitswap_outgoing_query_ids";
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

const BAN_PEER_DURATION: Duration = Duration::from_secs(60 * 60); //1h

type HelloRequestTable =
    HashMap<RequestId, OneShotSender<Result<HelloResponse, RequestResponseError>>>;

type CxRequestTable =
    HashMap<RequestId, OneShotSender<Result<ChainExchangeResponse, RequestResponseError>>>;

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
    BitswapRequestOutbound {
        query_id: libp2p_bitswap::QueryId,
        cid: Cid,
    },
    BitswapResponseInbound {
        query_id: libp2p_bitswap::QueryId,
        cid: Cid,
    },
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
        response_channel: OneShotSender<Result<ChainExchangeResponse, RequestResponseError>>,
    },
    HelloRequest {
        peer_id: PeerId,
        request: HelloRequest,
        response_channel: OneShotSender<Result<HelloResponse, RequestResponseError>>,
    },
    BitswapRequest {
        cid: Cid,
        response_channel: OneShotSender<()>,
    },
    JSONRPCRequest {
        method: NetRPCMethods,
    },
}

/// Network RPC API methods used to gather data from libp2p node.
#[derive(Debug)]
pub enum NetRPCMethods {
    NetAddrsListen(OneShotSender<(PeerId, Vec<Multiaddr>)>),
    NetPeers(OneShotSender<HashMap<PeerId, Vec<Multiaddr>>>),
    NetConnect(OneShotSender<bool>, PeerId, Vec<Multiaddr>),
    NetDisconnect(OneShotSender<()>, PeerId),
}

/// The `Libp2pService` listens to events from the libp2p swarm.
pub struct Libp2pService<DB, P: StoreParams> {
    config: Libp2pConfig,
    swarm: Swarm<ForestBehaviour<P>>,
    cs: Arc<ChainStore<DB>>,
    peer_manager: Arc<PeerManager>,
    network_receiver_in: flume::Receiver<NetworkMessage>,
    network_sender_in: Sender<NetworkMessage>,
    network_receiver_out: flume::Receiver<NetworkEvent>,
    network_sender_out: Sender<NetworkEvent>,
    network_name: String,
    genesis_cid: Cid,
}

impl<DB, P: StoreParams> Libp2pService<DB, P>
where
    DB: Blockstore + Store + BitswapStore<Params = P> + Clone + Sync + Send + 'static,
{
    pub fn new(
        config: Libp2pConfig,
        cs: Arc<ChainStore<DB>>,
        peer_manager: Arc<PeerManager>,
        net_keypair: Keypair,
        network_name: &str,
        genesis_cid: Cid,
    ) -> Self {
        let peer_id = PeerId::from(net_keypair.public());

        let transport = build_transport(net_keypair.clone());

        let limits = ConnectionLimits::default()
            .with_max_pending_incoming(Some(10))
            .with_max_pending_outgoing(Some(30))
            .with_max_established_incoming(Some(config.target_peer_count))
            .with_max_established_outgoing(Some(config.target_peer_count))
            .with_max_established_per_peer(Some(5));

        let mut swarm = SwarmBuilder::with_tokio_executor(
            transport,
            ForestBehaviour::new(&net_keypair, &config, network_name, cs.db.clone()),
            peer_id,
        )
        .connection_limits(limits)
        .notify_handler_buffer_size(std::num::NonZeroUsize::new(20).expect("Not zero"))
        .connection_event_buffer_size(64)
        .build();

        // Subscribe to gossipsub topics with the network name suffix
        for topic in PUBSUB_TOPICS.iter() {
            let t = Topic::new(format!("{topic}/{network_name}"));
            swarm.behaviour_mut().subscribe(&t).unwrap();
        }

        let (network_sender_in, network_receiver_in) = flume::unbounded();
        let (network_sender_out, network_receiver_out) = flume::unbounded();

        Libp2pService {
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
        }
    }

    /// Starts the libp2p service networking stack. This Future resolves when shutdown occurs.
    pub async fn run(mut self) -> anyhow::Result<()> {
        info!("Running libp2p service");
        Swarm::listen_on(&mut self.swarm, self.config.listening_multiaddr)?;
        // Bootstrap with Kademlia
        if let Err(e) = self.swarm.behaviour_mut().bootstrap() {
            warn!("Failed to bootstrap with Kademlia: {e}");
        }

        let mut swarm_stream = self.swarm.fuse();
        let mut network_stream = self.network_receiver_in.stream().fuse();
        let mut interval =
            IntervalStream::new(tokio::time::interval(Duration::from_secs(15))).fuse();
        let pubsub_block_str = format!("{}/{}", PUBSUB_BLOCK_STR, self.network_name);
        let pubsub_msg_str = format!("{}/{}", PUBSUB_MSG_STR, self.network_name);

        let mut hello_request_table = HashMap::new();
        let mut cx_request_table = HashMap::new();
        let mut outgoing_bitswap_query_ids = HashMap::new();
        let (cx_response_tx, cx_response_rx) = flume::unbounded();
        let mut cx_response_rx_stream = cx_response_rx.stream().fuse();
        let mut peer_ops_rx_stream = self.peer_manager.peer_ops_rx().stream().fuse();
        let mut libp2p_registry = Default::default();
        let metrics = Metrics::new(&mut libp2p_registry);
        forest_metrics::add_metrics_registry("libp2p".into(), libp2p_registry).await;
        loop {
            select! {
                swarm_event = swarm_stream.next() => match swarm_event {
                    // outbound events
                    Some(SwarmEvent::Behaviour(event)) => {
                        metrics.record(&event);
                        handle_forest_behaviour_event(
                            swarm_stream.get_mut(),
                            &self.peer_manager,
                            event,
                            &self.cs,
                            &self.genesis_cid,
                            &self.network_sender_out,
                            &mut hello_request_table,
                            &mut cx_request_table,
                            &mut outgoing_bitswap_query_ids,
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
                            message,
                            &self.network_sender_out,
                            &mut hello_request_table,
                            &mut cx_request_table,
                            &mut outgoing_bitswap_query_ids).await;
                    }
                    None => { break; }
                },
                interval_event = interval.next() => if interval_event.is_some() {
                    // Print peer count on an interval.
                    debug!("Peers connected: {}", swarm_stream.get_mut().behaviour_mut().peers().len());
                },
                pair_opt = cx_response_rx_stream.next() => {
                    if let Some((_request_id, channel, cx_response)) = pair_opt {
                        let behaviour = swarm_stream.get_mut().behaviour_mut();
                        if let Err(e) = behaviour.chain_exchange.send_response(channel, cx_response) {
                            warn!("Error sending chain exchange response: {e:?}");
                        }
                    }
                },
                peer_ops_opt = peer_ops_rx_stream.next() => {
                    if let Some(peer_ops) = peer_ops_opt {
                        handle_peer_ops(swarm_stream.get_mut(), peer_ops);
                    }
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

fn handle_peer_ops<P: StoreParams>(swarm: &mut Swarm<ForestBehaviour<P>>, peer_ops: PeerOperation) {
    use PeerOperation::*;
    match peer_ops {
        Ban(peer_id, reason) => {
            warn!("Banning {peer_id}, reason: {reason}");
            swarm.ban_peer_id(peer_id);
        }
        Unban(peer_id) => {
            info!("Unbanning {peer_id}");
            swarm.unban_peer_id(peer_id);
        }
    }
}

async fn handle_network_message<P: StoreParams>(
    swarm: &mut Swarm<ForestBehaviour<P>>,
    message: NetworkMessage,
    network_sender_out: &Sender<NetworkEvent>,
    hello_request_table: &mut HelloRequestTable,
    cx_request_table: &mut CxRequestTable,
    outgoing_bitswap_query_ids: &mut HashMap<libp2p_bitswap::QueryId, Cid>,
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
            let request_id = swarm.behaviour_mut().hello.send_request(&peer_id, request);
            hello_request_table.insert(request_id, response_channel);
            metrics::NETWORK_CONTAINER_CAPACITIES
                .with_label_values(&[metrics::values::HELLO_REQUEST_TABLE])
                .set(hello_request_table.capacity() as u64);
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
            let request_id = swarm
                .behaviour_mut()
                .chain_exchange
                .send_request(&peer_id, request);
            cx_request_table.insert(request_id, response_channel);
            metrics::NETWORK_CONTAINER_CAPACITIES
                .with_label_values(&[metrics::values::CX_REQUEST_TABLE])
                .set(cx_request_table.capacity() as u64);
            emit_event(
                network_sender_out,
                NetworkEvent::ChainExchangeRequestOutbound { request_id },
            )
            .await;
        }
        NetworkMessage::BitswapRequest {
            cid,
            response_channel: _,
        } => match swarm.behaviour_mut().want_block(cid) {
            Ok(query_id) => {
                outgoing_bitswap_query_ids.insert(query_id, cid);
                metrics::NETWORK_CONTAINER_CAPACITIES
                    .with_label_values(&[metrics::values::BITSWAP_OUTGOING_QUERY_IDS])
                    .set(outgoing_bitswap_query_ids.capacity() as u64);
                emit_event(
                    network_sender_out,
                    NetworkEvent::BitswapRequestOutbound { query_id, cid },
                )
                .await;
            }
            Err(e) => warn!("Failed to send a bitswap want_block: {}", e.to_string()),
        },
        NetworkMessage::JSONRPCRequest { method } => match method {
            NetRPCMethods::NetAddrsListen(response_channel) => {
                let listeners: Vec<_> = Swarm::listeners(swarm).cloned().collect();
                let peer_id = Swarm::local_peer_id(swarm);

                if response_channel.send((*peer_id, listeners)).is_err() {
                    warn!("Failed to get Libp2p listeners");
                }
            }
            NetRPCMethods::NetPeers(response_channel) => {
                let peer_addresses: &HashMap<PeerId, Vec<Multiaddr>> =
                    swarm.behaviour_mut().peer_addresses();

                if response_channel.send(peer_addresses.to_owned()).is_err() {
                    warn!("Failed to get Libp2p peers");
                }
            }
            NetRPCMethods::NetConnect(response_channel, peer_id, mut addresses) => {
                let mut success = false;

                for multiaddr in addresses.iter_mut() {
                    multiaddr.push(Protocol::P2p(
                        Multihash::from_bytes(&peer_id.to_bytes()).unwrap(),
                    ));

                    if Swarm::dial(swarm, multiaddr.clone()).is_ok() {
                        success = true;
                        break;
                    };
                }

                if response_channel.send(success).is_err() {
                    warn!("Failed to connect to a peer");
                }
            }
            NetRPCMethods::NetDisconnect(response_channel, peer_id) => {
                let _ = Swarm::disconnect_peer_id(swarm, peer_id);
                if response_channel.send(()).is_err() {
                    warn!("Failed to disconnect from a peer");
                }
            }
        },
    }
}

async fn handle_discovery_event<P: StoreParams>(
    discovery_out: DiscoveryOut,
    swarm: &mut Swarm<ForestBehaviour<P>>,
    network_sender_out: &Sender<NetworkEvent>,
) {
    let behaviour = swarm.behaviour_mut();
    match discovery_out {
        DiscoveryOut::Connected(peer_id, addresses) => {
            debug!("Peer connected, {:?}", peer_id);
            for addr in addresses {
                behaviour.bitswap.add_address(&peer_id, addr);
            }
            emit_event(network_sender_out, NetworkEvent::PeerConnected(peer_id)).await;
        }
        DiscoveryOut::Disconnected(peer_id, addresses) => {
            debug!("Peer disconnected, {:?}", peer_id);
            for addr in addresses {
                behaviour.bitswap.remove_address(&peer_id, &addr);
            }
            emit_event(network_sender_out, NetworkEvent::PeerDisconnected(peer_id)).await;
        }
    }
}

async fn handle_gossip_event(
    e: GossipsubEvent,
    network_sender_out: &Sender<NetworkEvent>,
    pubsub_block_str: &str,
    pubsub_msg_str: &str,
) {
    if let GossipsubEvent::Message {
        propagation_source: source,
        message,
        message_id: _,
    } = e
    {
        let topic = message.topic.as_str();
        let message = message.data;
        trace!("Got a Gossip Message from {:?}", source);
        if topic == pubsub_block_str {
            match from_slice::<GossipBlock>(&message) {
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
            match from_slice::<SignedMessage>(&message) {
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

async fn handle_hello_event<P: StoreParams>(
    rr_event: RequestResponseEvent<HelloRequest, HelloResponse, HelloResponse>,
    swarm: &mut Swarm<ForestBehaviour<P>>,
    peer_manager: &Arc<PeerManager>,
    genesis_cid: &Cid,
    network_sender_out: &Sender<NetworkEvent>,
    hello_request_table: &mut HelloRequestTable,
) {
    let behaviour = swarm.behaviour_mut();
    match rr_event {
        RequestResponseEvent::Message { peer, message } => match message {
            RequestResponseMessage::Request {
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
                    if let Err(e) = behaviour
                        .hello
                        .send_response(channel, HelloResponse { arrival, sent })
                    {
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
            RequestResponseMessage::Response {
                request_id,
                response,
            } => {
                // Send the successful response through channel out.
                if let Some(tx) = hello_request_table.remove(&request_id) {
                    metrics::NETWORK_CONTAINER_CAPACITIES
                        .with_label_values(&[metrics::values::HELLO_REQUEST_TABLE])
                        .set(hello_request_table.capacity() as u64);
                    if tx.send(Ok(response)).is_err() {
                        warn!("Fail to send Hello response");
                    } else {
                        emit_event(
                            network_sender_out,
                            NetworkEvent::HelloResponseInbound { request_id },
                        )
                        .await;
                    }
                } else {
                    warn!("RPCResponse receive failed: channel not found");
                };
            }
        },
        RequestResponseEvent::OutboundFailure {
            peer,
            request_id,
            error,
        } => {
            debug!(
                "Hello outbound error (peer: {:?}) (id: {:?}): {:?}",
                peer, request_id, error
            );

            // Send error through channel out.
            let tx = hello_request_table.remove(&request_id);
            if let Some(tx) = tx {
                metrics::NETWORK_CONTAINER_CAPACITIES
                    .with_label_values(&[metrics::values::HELLO_REQUEST_TABLE])
                    .set(hello_request_table.capacity() as u64);
                if tx.send(Err(error.into())).is_err() {
                    warn!("RPCResponse receive failed");
                }
            }
        }
        RequestResponseEvent::InboundFailure {
            peer,
            error,
            request_id: _,
        } => {
            debug!("Hello inbound error (peer: {:?}): {:?}", peer, error);
        }
        RequestResponseEvent::ResponseSent { .. } => (),
    }
}

async fn handle_bitswap_event(
    bs_event: BitswapEvent,
    network_sender_out: &Sender<NetworkEvent>,
    outgoing_bitswap_query_ids: &mut HashMap<libp2p_bitswap::QueryId, Cid>,
) {
    let get_prefix = |query_id: &libp2p_bitswap::QueryId| {
        if outgoing_bitswap_query_ids.contains_key(query_id) {
            "Outgoing"
        } else {
            "Inbound"
        }
    };
    match bs_event {
        BitswapEvent::Progress(query_id, num_missing) => {
            let prefix = get_prefix(&query_id);
            debug!("{prefix} bitswap query {query_id} in progress, {num_missing} blocks pending");
        }
        BitswapEvent::Complete(query_id, result) => match result {
            Ok(()) => {
                let prefix = get_prefix(&query_id);
                debug!("{prefix} bitswap query {query_id} completed successfully");
                if let Some(cid) = outgoing_bitswap_query_ids.remove(&query_id) {
                    metrics::NETWORK_CONTAINER_CAPACITIES
                        .with_label_values(&[metrics::values::BITSWAP_OUTGOING_QUERY_IDS])
                        .set(outgoing_bitswap_query_ids.capacity() as u64);
                    emit_event(
                        network_sender_out,
                        NetworkEvent::BitswapResponseInbound { query_id, cid },
                    )
                    .await;
                }
            }
            Err(err) => {
                let prefix = get_prefix(&query_id);
                let msg = format!("{prefix} bitswap query {query_id} completed with error: {err}");
                if outgoing_bitswap_query_ids.contains_key(&query_id) {
                    warn!("{msg}");
                } else {
                    debug!("{msg}");
                }
            }
        },
    }
}

async fn handle_ping_event(ping_event: ping::Event, peer_manager: &Arc<PeerManager>) {
    match ping_event.result {
        Ok(ping::Success::Ping { rtt }) => {
            trace!(
                "PingSuccess::Ping rtt to {} is {} ms",
                ping_event.peer.to_base58(),
                rtt.as_millis()
            );
        }
        Ok(ping::Success::Pong) => {
            trace!("PingSuccess::Pong from {}", ping_event.peer.to_base58());
        }
        Err(ping::Failure::Other { error }) => {
            warn!(
                "PingFailure::Other {}: {}",
                ping_event.peer.to_base58(),
                error
            );
        }
        Err(err) => {
            let err = err.to_string();
            let peer = ping_event.peer.to_base58();
            warn!("{err}: {peer}",);
            if err.contains("protocol not supported") {
                peer_manager
                    .ban_peer(
                        ping_event.peer,
                        format!("Ping protocol err: {err}"),
                        Some(BAN_PEER_DURATION),
                    )
                    .await;
            }
        }
    }
}

async fn handle_chain_exchange_event<DB, P: StoreParams>(
    ce_event: RequestResponseEvent<ChainExchangeRequest, ChainExchangeResponse>,
    db: &Arc<ChainStore<DB>>,
    network_sender_out: &Sender<NetworkEvent>,
    cx_request_table: &mut CxRequestTable,
    cx_response_tx: Sender<(
        RequestId,
        ResponseChannel<ChainExchangeResponse>,
        ChainExchangeResponse,
    )>,
) where
    DB: Blockstore + Store + BitswapStore<Params = P> + Clone + Sync + Send + 'static,
{
    match ce_event {
        RequestResponseEvent::Message { peer, message } => {
            match message {
                RequestResponseMessage::Request {
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
                RequestResponseMessage::Response {
                    request_id,
                    response,
                } => {
                    emit_event(
                        network_sender_out,
                        NetworkEvent::ChainExchangeResponseInbound { request_id },
                    )
                    .await;
                    let tx = cx_request_table.remove(&request_id);
                    // Send the successful response through channel out.
                    if let Some(tx) = tx {
                        metrics::NETWORK_CONTAINER_CAPACITIES
                            .with_label_values(&[metrics::values::CX_REQUEST_TABLE])
                            .set(cx_request_table.capacity() as u64);
                        if tx.send(Ok(response)).is_err() {
                            debug!("Failed to send ChainExchange response")
                        }
                    } else {
                        warn!("RPCResponse receive failed: channel not found");
                    };
                }
            }
        }
        RequestResponseEvent::OutboundFailure {
            peer,
            request_id,
            error,
        } => {
            warn!(
                "ChainExchange outbound error (peer: {:?}) (id: {:?}): {:?}",
                peer, request_id, error
            );

            let tx = cx_request_table.remove(&request_id);

            // Send error through channel out.
            if let Some(tx) = tx {
                metrics::NETWORK_CONTAINER_CAPACITIES
                    .with_label_values(&[metrics::values::CX_REQUEST_TABLE])
                    .set(cx_request_table.capacity() as u64);
                if tx.send(Err(error.into())).is_err() {
                    warn!("RPCResponse receive failed")
                }
            }
        }
        RequestResponseEvent::InboundFailure {
            peer,
            error,
            request_id: _,
        } => {
            debug!(
                "ChainExchange inbound error (peer: {:?}): {:?}",
                peer, error
            );
        }
        RequestResponseEvent::ResponseSent { request_id, .. } => {
            emit_event(
                network_sender_out,
                NetworkEvent::ChainExchangeResponseOutbound { request_id },
            )
            .await;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_forest_behaviour_event<DB, P: StoreParams>(
    swarm: &mut Swarm<ForestBehaviour<P>>,
    peer_manager: &Arc<PeerManager>,
    event: ForestBehaviourEvent<P>,
    db: &Arc<ChainStore<DB>>,
    genesis_cid: &Cid,
    network_sender_out: &Sender<NetworkEvent>,
    hello_request_table: &mut HelloRequestTable,
    cx_request_table: &mut CxRequestTable,
    outgoing_bitswap_query_ids: &mut HashMap<libp2p_bitswap::QueryId, Cid>,
    cx_response_tx: Sender<(
        RequestId,
        ResponseChannel<ChainExchangeResponse>,
        ChainExchangeResponse,
    )>,
    pubsub_block_str: &str,
    pubsub_msg_str: &str,
) where
    DB: Blockstore + Store + BitswapStore<Params = P> + Clone + Sync + Send + 'static,
{
    match event {
        ForestBehaviourEvent::Discovery(discovery_out) => {
            handle_discovery_event(discovery_out, swarm, network_sender_out).await
        }
        ForestBehaviourEvent::Gossipsub(e) => {
            handle_gossip_event(e, network_sender_out, pubsub_block_str, pubsub_msg_str).await
        }
        ForestBehaviourEvent::Hello(rr_event) => {
            handle_hello_event(
                rr_event,
                swarm,
                peer_manager,
                genesis_cid,
                network_sender_out,
                hello_request_table,
            )
            .await
        }
        ForestBehaviourEvent::Bitswap(bs_event) => {
            handle_bitswap_event(bs_event, network_sender_out, outgoing_bitswap_query_ids).await
        }
        ForestBehaviourEvent::Ping(ping_event) => handle_ping_event(ping_event, peer_manager).await,
        ForestBehaviourEvent::Identify(_) => {}
        ForestBehaviourEvent::ChainExchange(ce_event) => {
            handle_chain_exchange_event(
                ce_event,
                db,
                network_sender_out,
                cx_request_table,
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

/// Builds the transport stack that libp2p will communicate over.
pub fn build_transport(local_key: Keypair) -> Boxed<(PeerId, StreamMuxerBox)> {
    let tcp_transport =
        || libp2p::tcp::tokio::Transport::new(libp2p::tcp::Config::new().nodelay(true));
    let transport = libp2p::dns::TokioDnsConfig::system(tcp_transport()).unwrap();
    let auth_config = {
        let dh_keys = noise::Keypair::<noise::X25519Spec>::new()
            .into_authentic(&local_key)
            .expect("Noise key generation failed");

        noise::NoiseConfig::xx(dh_keys).into_authenticated()
    };

    let mplex_config = {
        let mut mplex_config = mplex::MplexConfig::new();
        mplex_config.set_max_buffer_size(usize::MAX);

        let mut yamux_config = yamux::YamuxConfig::default();
        yamux_config.set_max_buffer_size(16 * 1024 * 1024);
        yamux_config.set_receive_window_size(16 * 1024 * 1024);
        // yamux_config.set_window_update_mode(WindowUpdateMode::OnRead);
        core::upgrade::SelectUpgrade::new(yamux_config, mplex_config)
    };

    transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(auth_config)
        .multiplex(mplex_config)
        .timeout(Duration::from_secs(20))
        .boxed()
}

/// Fetch key-pair from disk, returning none if it cannot be decoded.
pub fn get_keypair(path: &Path) -> Option<Keypair> {
    match read_file_to_vec(path) {
        Err(e) => {
            info!("Networking keystore not found!");
            trace!("Error {:?}", e);
            None
        }
        Ok(mut vec) => match ed25519::Keypair::decode(&mut vec) {
            Ok(kp) => {
                info!("Recovered libp2p keypair from {:?}", &path);
                Some(Keypair::Ed25519(kp))
            }
            Err(e) => {
                info!("Could not decode networking keystore!");
                trace!("Error {:?}", e);
                None
            }
        },
    }
}
