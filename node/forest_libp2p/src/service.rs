// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::blocksync::{BlockSyncRequest, BlockSyncResponse};
use super::rpc::RPCRequest;
use super::{ForestBehaviour, ForestBehaviourEvent, Libp2pConfig};
use crate::hello::{HelloRequest, HelloResponse};
use async_std::stream;
use async_std::sync::{channel, Receiver, Sender};
use forest_cid::{multihash::Blake2b256, Cid};
use futures::channel::oneshot::Sender as OneShotSender;
use futures::select;
use futures_util::stream::StreamExt;
use ipld_blockstore::BlockStore;
use libp2p::{
    core,
    core::muxing::StreamMuxerBox,
    core::transport::boxed::Boxed,
    gossipsub::TopicHash,
    identity::{ed25519, Keypair},
    mplex, secio, yamux, PeerId, Swarm, Transport,
};
use libp2p_request_response::{RequestId, ResponseChannel};
use log::{debug, info, trace, warn};
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::sync::Arc;
use std::time::Duration;
use utils::read_file_to_vec;

pub use libp2p::gossipsub::Topic;

pub const PUBSUB_BLOCK_STR: &str = "/fil/blocks";
pub const PUBSUB_MSG_STR: &str = "/fil/msgs";

const PUBSUB_TOPICS: [&str; 2] = [PUBSUB_BLOCK_STR, PUBSUB_MSG_STR];

/// Events emitted by this Service
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    PubsubMessage {
        source: Option<PeerId>,
        topics: Vec<TopicHash>,
        message: Vec<u8>,
    },
    HelloRequest {
        request: HelloRequest,
        channel: ResponseChannel<HelloResponse>,
    },
    HelloResponse {
        request_id: RequestId,
        response: HelloResponse,
    },
    BlockSyncRequest {
        request: BlockSyncRequest,
        channel: ResponseChannel<BlockSyncResponse>,
    },
    BlockSyncResponse {
        request_id: RequestId,
        response: BlockSyncResponse,
    },
    PeerDialed {
        peer_id: PeerId,
    },
    BitswapBlock {
        cid: Cid,
    },
}

/// Events into this Service
#[derive(Debug)]
pub enum NetworkMessage {
    PubsubMessage {
        topic: Topic,
        message: Vec<u8>,
    },
    BlockSyncRequest {
        peer_id: PeerId,
        request: BlockSyncRequest,
        response_channel: OneShotSender<BlockSyncResponse>,
    },
    HelloRequest {
        peer_id: PeerId,
        request: HelloRequest,
    },
}
/// The Libp2pService listens to events from the Libp2p swarm.
pub struct Libp2pService<DB: BlockStore> {
    pub swarm: Swarm<ForestBehaviour>,
    db: Arc<DB>,
    /// Keeps track of Blocksync requests to responses
    bs_request_table: HashMap<RequestId, OneShotSender<BlockSyncResponse>>,
    network_receiver_in: Receiver<NetworkMessage>,
    network_sender_in: Sender<NetworkMessage>,
    network_receiver_out: Receiver<NetworkEvent>,
    network_sender_out: Sender<NetworkEvent>,
}

impl<DB> Libp2pService<DB>
where
    DB: BlockStore,
{
    /// Constructs a Libp2pService
    pub fn new(
        config: Libp2pConfig,
        db: Arc<DB>,
        net_keypair: Keypair,
        network_name: &str,
    ) -> Self {
        let peer_id = PeerId::from(net_keypair.public());

        let transport = build_transport(net_keypair.clone());

        let mut swarm = {
            let be = ForestBehaviour::new(&net_keypair, &config, network_name);
            Swarm::new(transport, be, peer_id)
        };

        Swarm::listen_on(&mut swarm, config.listening_multiaddr).unwrap();

        // Subscribe to gossipsub topics with the network name suffix
        for topic in PUBSUB_TOPICS.iter() {
            swarm.subscribe(Topic::new(format!("{}/{}", topic, network_name)));
        }

        // Bootstrap with Kademlia
        if let Err(e) = swarm.bootstrap() {
            warn!("Failed to bootstrap with Kademlia: {}", e);
        }

        let (network_sender_in, network_receiver_in) = channel(20);
        let (network_sender_out, network_receiver_out) = channel(20);

        Libp2pService {
            swarm,
            db,
            bs_request_table: HashMap::new(),
            network_receiver_in,
            network_sender_in,
            network_receiver_out,
            network_sender_out,
        }
    }

    /// Starts the `Libp2pService` networking stack. This Future resolves when shutdown occurs.
    pub async fn run(mut self) {
        let mut swarm_stream = self.swarm.fuse();
        let mut network_stream = self.network_receiver_in.fuse();
        let mut interval = stream::interval(Duration::from_secs(10)).fuse();

        loop {
            select! {
                swarm_event = swarm_stream.next() => match swarm_event {
                    Some(event) => match event {
                        ForestBehaviourEvent::PeerDialed(peer_id) => {
                            debug!("Peer dialed, {:?}", peer_id);
                            self.network_sender_out.send(NetworkEvent::PeerDialed{
                                peer_id
                            }).await;
                        }
                        ForestBehaviourEvent::PeerDisconnected(peer_id) => {
                            debug!("Peer disconnected, {:?}", peer_id);
                        }
                        ForestBehaviourEvent::GossipMessage {
                            source,
                            topics,
                            message,
                        } => {
                            debug!("Got a Gossip Message from {:?}", source);
                            self.network_sender_out.send(NetworkEvent::PubsubMessage {
                                source,
                                topics,
                                message
                            }).await;
                        }
                        ForestBehaviourEvent::HelloRequest { request, channel, .. } => {
                            debug!("Received hello request: {:?}", request);
                            self.network_sender_out.send(NetworkEvent::HelloRequest {
                                request,
                                channel,
                            }).await;
                        }
                        ForestBehaviourEvent::HelloResponse { request_id, response, .. } => {
                            debug!("Received hello response (id: {:?}): {:?}", request_id, response);
                            self.network_sender_out.send(NetworkEvent::HelloResponse {
                                request_id,
                                response,
                            }).await;
                        }
                        ForestBehaviourEvent::BlockSyncRequest { channel, .. } => {
                            // TODO implement blocksync provider
                            let _ = channel.send(BlockSyncResponse {
                                chain: vec![],
                                status: 203,
                                message: "handling requests not implemented".to_owned(),
                            });
                        }
                        ForestBehaviourEvent::BlockSyncResponse { request_id, response, .. } => {
                            debug!("Received blocksync response (id: {:?}): {:?}", request_id, response);
                            let tx = self.bs_request_table.remove(&request_id);

                            if let Some(tx) = tx {
                                if let Err(e) = tx.send(response) {
                                    debug!("RPCResponse receive failed: {:?}", e)
                                }
                            }
                            else {
                                debug!("RPCResponse receive failed: channel not found");
                            };
                        }
                        ForestBehaviourEvent::BitswapReceivedBlock(peer_id, cid, block) => {
                            let res: Result<_, String> = self.db.put(&block, Blake2b256).map_err(|e| e.to_string());
                            match res {
                                Ok(actual_cid) => {
                                    if actual_cid != cid {
                                        warn!("Bitswap cid mismatch: cid {:?}, expected cid: {:?}", actual_cid, cid);
                                    } else {
                                        trace!("saved bitswap block with cid {:?}", cid);
                                    }
                                    self.network_sender_out.send(NetworkEvent::BitswapBlock{cid}).await;
                                }
                                Err(e) => {
                                    warn!("failed to save bitswap block: {:?}", e.to_string());
                                }
                            }
                        },
                        ForestBehaviourEvent::BitswapReceivedWant(peer_id, cid,) =>  match self.db.get(&cid) {
                            Ok(Some(data)) => {
                                match swarm_stream.get_mut().send_block(&peer_id, cid, data) {
                                    Ok(_) => trace!("Sent bitswap message successfully"),
                                    Err(e) => warn!("Failed to send Bitswap reply: {}", e.to_string()),
                                }
                            }
                            Ok(None) => {
                                trace!("Don't have data for: {}", cid);
                            }
                            Err(e) => {
                                trace!("Failed to get data: {}", e.to_string());
                            }
                        },
                    }
                    None => { break; }
                },
                rpc_message = network_stream.next() => match rpc_message {
                    Some(message) =>  match message {
                        NetworkMessage::PubsubMessage { topic, message } => {
                            if let Err(e) = swarm_stream.get_mut().publish(&topic, message) {
                                warn!("Failed to send gossipsub message: {:?}", e);
                            }
                        }
                        NetworkMessage::HelloRequest { peer_id, request } => {
                            let _ = swarm_stream.get_mut().send_rpc_request(&peer_id, RPCRequest::Hello(request));
                        }
                        NetworkMessage::BlockSyncRequest { peer_id, request, response_channel } => {
                            let id = swarm_stream.get_mut().send_rpc_request(&peer_id, RPCRequest::BlockSync(request));
                            self.bs_request_table.insert(id, response_channel);
                        }
                    }
                    None => { break; }
                },
                interval_event = interval.next() => if interval_event.is_some() {
                    info!("Peers connected: {}", swarm_stream.get_ref().peers().len());
                }
            };
        }
    }

    /// Returns a `Sender` allowing you to send messages over GossipSub
    pub fn network_sender(&self) -> Sender<NetworkMessage> {
        self.network_sender_in.clone()
    }

    /// Returns a `Receiver` to listen to network events
    pub fn network_receiver(&self) -> Receiver<NetworkEvent> {
        self.network_receiver_out.clone()
    }
}

/// Builds the transport stack that LibP2P will communicate over
pub fn build_transport(local_key: Keypair) -> Boxed<(PeerId, StreamMuxerBox), Error> {
    let transport = libp2p::tcp::TcpConfig::new().nodelay(true);
    let transport = libp2p::dns::DnsConfig::new(transport).unwrap();
    transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(secio::SecioConfig::new(local_key))
        .multiplex(core::upgrade::SelectUpgrade::new(
            yamux::Config::default(),
            mplex::MplexConfig::new(),
        ))
        .map(|(peer, muxer), _| (peer, core::muxing::StreamMuxerBox::new(muxer)))
        .timeout(Duration::from_secs(20))
        .map_err(|err| Error::new(ErrorKind::Other, err))
        .boxed()
}

/// Fetch keypair from disk, returning none if it cannot be decoded
pub fn get_keypair(path: &str) -> Option<Keypair> {
    match read_file_to_vec(&path) {
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
