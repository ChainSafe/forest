use super::{MAINNET_PREFIX, TESTNET_PREFIX};

/// Network defines the preconfigured networks to use with address encoding
pub enum Network {
    Mainnet,
    Testnet,
}

impl Network {
    /// to_prefix is used to convert the network into a string
    /// used when converting address to string
    pub(super) fn to_prefix(&self) -> &'static str {
        match self {
            Network::Mainnet => MAINNET_PREFIX,
            Network::Testnet => TESTNET_PREFIX,
        }
    }
}
