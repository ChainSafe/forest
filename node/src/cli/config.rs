use serde_derive::Deserialize;

use ferret_libp2p::config::Libp2pConfig;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub network: Libp2pConfig,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            network: Libp2pConfig::default(),
        }
    }
}
