#![cfg(test)]

use forest_libp2p::service::Libp2pService;
use forest_libp2p::config::Libp2pConfig;
use async_std::task;
use libp2p::swarm::Swarm;
use crate::log;
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

fn build_node_pair () -> (Libp2pService, Libp2pService) {
    let log = setup_logging();
    let mut config1 = Libp2pConfig::default();
    let mut config2 = Libp2pConfig::default();
    config1.listening_multiaddr = "/ip4/0.0.0.0/tcp/10005".to_owned();
    config2.listening_multiaddr = "/ip4/0.0.0.0/tcp/10006".to_owned();


    let mut lp2p_service1 = Libp2pService::new(log.clone(), &config1);
    let mut lp2p_service2 = Libp2pService::new(log.clone(), &config2);
    // dial each other

    Swarm::dial_addr(&mut lp2p_service2.swarm, config1.listening_multiaddr.parse().unwrap());

    (lp2p_service1, lp2p_service2)

}

#[test]
fn test1() {
    let (lp2p1, lp2p2) = build_node_pair();
    task::block_on(async move{
        let han1 = task::spawn(async move {
            let tx
            lp2p1.run().await;
        });
        let han2 = task::spawn(async move {
            lp2p2.run().await;

        });
        han1.await;
        han2.await;
    });
}