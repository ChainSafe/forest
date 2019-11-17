use super::{MAINNET_PREFIX, TESTNET_PREFIX};

pub enum Network {
    Mainnet,
    Testnet,
}

impl Network {
    pub(crate) fn to_prefix(&self) -> &'static str {
        match self {
            Network::Mainnet => MAINNET_PREFIX,
            Network::Testnet => TESTNET_PREFIX,
        }
    }
}
