use num_derive::FromPrimitive;

/// Identifies the builtin actor types for usage with the
/// actor::resolve_builtin_actor_type syscall.
/// Note that there is a mirror of this enum in the FVM SDK src/actors/builtins.rs.
/// These must be kept in sync for the syscall to work correctly, without either side
/// importing the other.
#[derive(PartialEq, Eq, Clone, Copy, PartialOrd, Ord, FromPrimitive, Debug)]
#[repr(i32)]
pub enum Type {
    System = 1,
    Init = 2,
    Cron = 3,
    Account = 4,
    Power = 5,
    Miner = 6,
    Market = 7,
    PaymentChannel = 8,
    Multisig = 9,
    Reward = 10,
    VerifiedRegistry = 11,
    DataCap = 12,
}

impl Type {
    pub fn name(&self) -> &'static str {
        match *self {
            Type::System => "system",
            Type::Init => "init",
            Type::Cron => "cron",
            Type::Account => "account",
            Type::Power => "storagepower",
            Type::Miner => "storageminer",
            Type::Market => "storagemarket",
            Type::PaymentChannel => "paymentchannel",
            Type::Multisig => "multisig",
            Type::Reward => "reward",
            Type::VerifiedRegistry => "verifiedregistry",
            Type::DataCap => "datacap",
        }
    }
}
