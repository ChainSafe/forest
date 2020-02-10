#![cfg(test)]

use async_std::task;
use forest_libp2p::config::Libp2pConfig;
use forest_libp2p::rpc::{Message, RPCEvent, RPCRequest, RPCResponse, Response};
use forest_libp2p::service::{Libp2pService, NetworkMessage};
use libp2p::swarm::Swarm;
use slog::Drain;
use slog::*;
use slog_async;
use slog_term;

pub fn setup_logging() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    slog::Logger::root(drain, o!())
}

fn build_node_pair() -> (Libp2pService, Libp2pService) {
    let log = setup_logging();
    let mut config1 = Libp2pConfig::default();
    let mut config2 = Libp2pConfig::default();
    config1.listening_multiaddr = "/ip4/0.0.0.0/tcp/10005".to_owned();
    config2.listening_multiaddr = "/ip4/0.0.0.0/tcp/10006".to_owned();

    let lp2p_service1 = Libp2pService::new(log.clone(), &config1);
    let mut lp2p_service2 = Libp2pService::new(log.clone(), &config2);

    // dial each other

    Swarm::dial_addr(
        &mut lp2p_service2.swarm,
        "/ip4/127.0.0.1/tcp/10005".parse().unwrap(),
    )
    .unwrap();

    (lp2p_service1, lp2p_service2)
}

#[test]
fn test1() {
    let (sender, receiver) = build_node_pair();
    let sen_tx = sender.pubsub_sender();
    let _sen_rx = sender.pubsub_receiver();
    // let rec_tx = sender.pubsub_sender();
    // let rec_rx = sender.pubsub_receiver();

    let rpc_request = RPCEvent::Request(
        0,
        RPCRequest::BlocksyncRequest(Message {
            start: vec![],
            request_len: 0,
            options: 0,
        }),
    );

    let _rpc_response = RPCResponse::BlocksyncResponse(Response {
        chain: vec![],
        status: 1,
        message: "message".to_owned(),
    });

    let rpc_msg = NetworkMessage::RPCRequest {
        peer_id: Swarm::local_peer_id(&receiver.swarm).clone(),
        request: rpc_request.clone(),
    };

    task::block_on(async move {
        let han1 = task::spawn(async move {
            sender.run().await;
        });
        let han2 = task::spawn(async move {
            receiver.run().await;
        });

        std::thread::sleep(std::time::Duration::from_secs(2));
        sen_tx.send(rpc_msg).await;

        han1.await;
        han2.await;
    });
}
