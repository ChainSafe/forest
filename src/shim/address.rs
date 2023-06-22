// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
    str::FromStr,
};

use fvm_ipld_encoding::Cbor;
use fvm_shared::address::Address as Address_v2;
use fvm_shared3::address::Address as Address_v3;
pub use fvm_shared3::address::{Error, Network, Payload, Protocol, BLS_PUB_LEN, PAYLOAD_HASH_LEN};
use lazy_static::lazy_static;
use num_traits::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

// XXX: Copied from ref-fvm due to a bug in their definition.
lazy_static! {
    /// Zero address used to avoid allowing it to be used for verification.
    /// This is intentionally disallowed because it is an edge case with Filecoin's BLS
    /// signature verification.
    pub static ref ZERO_ADDRESS: Address_v3 = Network::Mainnet.parse_address("f3yaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaby2smx7a").unwrap();
}

static GLOBAL_NETWORK: AtomicU8 = AtomicU8::new(Network::Mainnet as u8);

thread_local! {
    // Thread local network identifier. Defaults to value in GLOBAL_NETWORK.
    static LOCAL_NETWORK: AtomicU8 = AtomicU8::new(GLOBAL_NETWORK.load(Ordering::Relaxed));
}

pub fn current_network() -> Network {
    FromPrimitive::from_u8(LOCAL_NETWORK.with(|ident| ident.load(Ordering::Relaxed)))
        .unwrap_or(Network::Mainnet)
}

fn current_global_network() -> Network {
    FromPrimitive::from_u8(GLOBAL_NETWORK.load(Ordering::Relaxed))
        .unwrap_or(Network::Mainnet)
}

/// Sets the default network.
///
/// The network is used to differentiate between different filecoin networks _in text_ but isn't
/// actually encoded in the binary representation of addresses. Changing the current network will:
///
/// 1. Change the prefix used when formatting an address as a string.
/// 2. Change the prefix _accepted_ when parsing a `StrictAddress`.
///
/// The current network is thread-local.
pub fn set_current_network(network: Network) {
    LOCAL_NETWORK.with(|ident| ident.store(network as u8, Ordering::Relaxed));
}

// Set global network identifier. The thread-local network inherits this value.
pub fn set_global_network(network: Network) {
    GLOBAL_NETWORK.store(network as u8, Ordering::Relaxed);
}

struct NetworkGuard(Network);
impl NetworkGuard {
    fn new(new_network: Network) -> Self {
        let previous_network = current_network();
        set_current_network(new_network);
        NetworkGuard(previous_network)
    }
}

impl Drop for NetworkGuard {
    fn drop(&mut self) {
        set_current_network(self.0);
    }
}

// Set the current network for the duration of a single function call. The current network may or
// may not be reset on panic.
pub fn with_network<X>(network: Network, cb: impl FnOnce() -> X) -> X {
    let guard = NetworkGuard::new(network);
    let result = cb();
    drop(guard);
    result
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Address(Address_v3);

impl Address {
    pub const SYSTEM_ACTOR: Address = Address::new_id(0);
    pub const INIT_ACTOR: Address = Address::new_id(1);
    pub const REWARD_ACTOR: Address = Address::new_id(2);
    pub const CRON_ACTOR: Address = Address::new_id(3);
    pub const POWER_ACTOR: Address = Address::new_id(4);
    pub const MARKET_ACTOR: Address = Address::new_id(5);
    pub const VERIFIED_REGISTRY_ACTOR: Address = Address::new_id(6);
    pub const DATACAP_TOKEN_ACTOR: Address = Address::new_id(7);
    pub const ETHEREUM_ACCOUNT_MANAGER_ACTOR: Address = Address::new_id(10);
    pub const RESERVE_ACTOR: Address = Address::new_id(90);
    pub const CHAOS_ACTOR: Address = Address::new_id(98);
    pub const BURNT_FUNDS_ACTOR: Address = Address::new_id(99);

    pub const fn new_id(id: u64) -> Self {
        Address(Address_v3::new_id(id))
    }

    pub fn new_actor(data: &[u8]) -> Self {
        Address(Address_v3::new_actor(data))
    }

    pub fn new_bls(pubkey: &[u8]) -> Result<Self, Error> {
        Address_v3::new_bls(pubkey).map(Address::from)
    }

    pub fn new_secp256k1(pubkey: &[u8]) -> Result<Self, Error> {
        Address_v3::new_secp256k1(pubkey).map(Address::from)
    }

    pub fn new_delegated(ns: u64, subaddress: &[u8]) -> Result<Self, Error> {
        Ok(Self(Address_v3::new_delegated(ns, subaddress)?))
    }

    pub fn protocol(&self) -> Protocol {
        self.0.protocol()
    }

    pub fn into_payload(self) -> Payload {
        self.0.into_payload()
    }

    pub fn from_bytes(bz: &[u8]) -> Result<Self, Error> {
        Address_v3::from_bytes(bz).map(Address)
    }
}

impl quickcheck::Arbitrary for Address {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Address(Address_v3::arbitrary(g))
    }
}

impl FromStr for Address {
    type Err = <Address_v3 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Network::Testnet
            .parse_address(s)
            .or_else(|_| Network::Mainnet.parse_address(s))
            .map(Address::from)
    }
}

impl Cbor for Address {}

use data_encoding::Encoding;
use data_encoding_macro::new_encoding;

use crate::GLOBAL;
/// defines the encoder for base32 encoding with the provided string with no padding
const ADDRESS_ENCODER: Encoding = new_encoding! {
    symbols: "abcdefghijklmnopqrstuvwxyz234567",
    padding: None,
};

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use fvm_shared3::address::CHECKSUM_HASH_LEN;
        const MAINNET_PREFIX: &str = "f";
        const TESTNET_PREFIX: &str = "t";

        let protocol = self.protocol();

        let prefix = if matches!(current_network(), Network::Mainnet) {
            MAINNET_PREFIX
        } else {
            TESTNET_PREFIX
        };

        // write `fP` where P is the protocol number.
        write!(f, "{}{}", prefix, protocol)?;

        fn write_payload(
            f: &mut std::fmt::Formatter<'_>,
            protocol: Protocol,
            prefix: Option<&[u8]>,
            data: &[u8],
        ) -> std::fmt::Result {
            let mut hasher = blake2b_simd::Params::new()
                .hash_length(CHECKSUM_HASH_LEN)
                .to_state();
            hasher.update(&[protocol as u8]);
            if let Some(prefix) = prefix {
                hasher.update(prefix);
            }
            hasher.update(data);

            let mut buf = Vec::with_capacity(data.len() + CHECKSUM_HASH_LEN);
            buf.extend(data);
            buf.extend(hasher.finalize().as_bytes());

            f.write_str(&ADDRESS_ENCODER.encode(&buf))
        }

        match self.payload() {
            Payload::ID(id) => write!(f, "{}", id),
            Payload::Secp256k1(data) | Payload::Actor(data) => {
                write_payload(f, protocol, None, data)
            }
            Payload::BLS(data) => write_payload(f, protocol, None, data),
            Payload::Delegated(addr) => {
                write!(f, "{}f", addr.namespace())?;
                write_payload(
                    f,
                    protocol,
                    Some(unsigned_varint::encode::u64(
                        addr.namespace(),
                        &mut unsigned_varint::encode::u64_buffer(),
                    )),
                    addr.subaddress(),
                )
            }
        }
    }
}

impl Deref for Address {
    type Target = Address_v3;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Address {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StrictAddress(Address);
impl Display for StrictAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for StrictAddress {
    type Err = <Address_v3 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let fvm_addr = current_network().parse_address(s)?;
        Ok(StrictAddress(fvm_addr.into()))
    }
}

// Conversion implementations.
// Note for `::from_bytes`. Both FVM2 and FVM3 addresses values as bytes must be
// identical and able to do a conversion, otherwise it is a logic error and
// Forest should not continue so there is no point in `TryFrom`.

impl From<Address> for StrictAddress {
    fn from(other: Address) -> Self {
        StrictAddress(other)
    }
}

impl From<StrictAddress> for Address {
    fn from(other: StrictAddress) -> Self {
        other.0
    }
}

impl From<Address_v3> for Address {
    fn from(other: Address_v3) -> Self {
        Address(other)
    }
}

impl From<Address_v2> for Address {
    fn from(other: Address_v2) -> Self {
        Address::from(
            Address_v3::from_bytes(&other.to_bytes())
                .expect("Couldn't convert between FVM2 and FVM3 addresses."),
        )
    }
}

impl From<&Address_v2> for Address {
    fn from(other: &Address_v2) -> Self {
        Address::from(
            Address_v3::from_bytes(&other.to_bytes())
                .expect("Couldn't convert between FVM2 and FVM3 addresses."),
        )
    }
}

impl From<&Address_v3> for Address {
    fn from(other: &Address_v3) -> Self {
        Address(*other)
    }
}

impl From<Address> for Address_v3 {
    fn from(other: Address) -> Self {
        other.0
    }
}

impl From<Address> for Address_v2 {
    fn from(other: Address) -> Address_v2 {
        Address_v2::from_bytes(&other.to_bytes())
            .expect("Couldn't convert between FVM2 and FVM3 addresses")
    }
}

impl From<&Address> for Address_v2 {
    fn from(other: &Address) -> Self {
        Address_v2::from_bytes(&other.to_bytes())
            .expect("Couldn't convert between FVM2 and FVM3 addresses")
    }
}

impl From<&Address> for Address_v3 {
    fn from(other: &Address) -> Self {
        other.0
    }
}

#[cfg(test)]
fn flip_network(input: Network) -> Network {
    match input {
        Network::Mainnet => Network::Testnet,
        Network::Testnet => Network::Mainnet,
    }
}

#[test]
fn relaxed_address_parsing() {
    assert!(Address::from_str("t01234").is_ok());
    assert!(Address::from_str("f01234").is_ok());
}

#[test]
fn strict_address_parsing() {
    with_network(Network::Mainnet, || {
        assert!(StrictAddress::from_str("f01234").is_ok());
        assert!(StrictAddress::from_str("t01234").is_err());
    });
    with_network(Network::Testnet, || {
        assert!(StrictAddress::from_str("f01234").is_err());
        assert!(StrictAddress::from_str("t01234").is_ok());
    });
}

#[test]
fn set_with_network() {
    let outer_network = current_network();
    let inner_network = flip_network(outer_network);
    with_network(inner_network, || {
        assert_eq!(current_network(), inner_network);
    });
    assert_eq!(outer_network, current_network());
}

#[test]
fn unwind_current_network_on_panic() {
    let outer_network = current_network();
    let inner_network = flip_network(outer_network);
    assert!(std::panic::catch_unwind(|| {
        with_network(inner_network, || {
            panic!("unwinding stack");
        })
    })
    .is_err());
    let new_outer_network = current_network();
    assert_eq!(outer_network, new_outer_network);
}

#[test]
fn inherit_global_network() {
    let outer_network = current_global_network();
    let inner_network = flip_network(outer_network);
    set_global_network(inner_network);
    std::thread::spawn(move || {
        assert_eq!(current_network(), inner_network);
    }).join().unwrap();
    set_global_network(outer_network);
}
