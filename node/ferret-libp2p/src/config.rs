use libp2p::gossipsub::Topic;

pub struct Libp2pConfig {
    pub listening_multiaddr: String,
    pub pubsub_topics: Vec<Topic>,
    pub bootstrap_peers: Vec<String>,
}

impl Default for Libp2pConfig {
    fn default() -> Self {
        Libp2pConfig {
            listening_multiaddr: "/ip4/0.0.0.0/tcp/0".to_owned(),
            pubsub_topics: vec![
                Topic::new("/fil/blocks".to_owned()),
                Topic::new("/fil/messages".to_owned()),
            ],
            bootstrap_peers: vec![],
        }
    }
}
