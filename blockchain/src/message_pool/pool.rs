// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use message::SignedMessage;

trait PoolValidator {
    // fn validate(&self, msg: SignedMessage);
}
#[allow(dead_code)]
// MessagePoolConfig holds all configuration options related to nodes message pool (mpool).
pub struct MessagePoolConfig {
    // max_pool_size is the maximum number of pending messages will will allow in the message pool at any time
    max_pool_size: u64,
    // max_nonce_gap is the maximum nonce of a message past the last received on chain
    max_nonce_gap: u64,
}
#[allow(dead_code)]
// Pool keeps an unordered, de-duplicated set of Messages and supports removal by CID.
// By 'de-duplicated' we mean that insertion of a message by cid that already
// exists is a nop. We use a Pool to store all messages received by this node
// via network or directly created via user command that have yet to be included
// in a block. Messages are removed as they are processed.
//
// Pool is safe for concurrent access.
pub struct Pool {
    cfg: MessagePoolConfig,
    validator: dyn PoolValidator,
    // pending CID messages
    // addressNonce
}
#[allow(dead_code)]
struct TimedMessage {
    // message
    added_at: u64,
}
#[allow(dead_code)]
fn new_default_message_pool_config() -> MessagePoolConfig {
    MessagePoolConfig {
        max_pool_size: 10000,
        max_nonce_gap: 100,
    }
}
#[allow(dead_code)]
// new_pool constructs a new Pool.
fn new_pool() -> &'static str {
    "Pool{ cfg: cfg, validator: validator}"
}
#[allow(dead_code)]
struct AddressNonce {
    //addr  address
    nonce: u64,
}
#[allow(dead_code)]
fn new_address_nonce(_msg: SignedMessage) -> &'static str {
    "return addressNonce"
}
#[allow(dead_code)]
// Add adds a message to the pool, tagged with the block height at which it was received.
// Does nothing if the message is already in the pool.
fn add(_msg: SignedMessage, _height: u64) -> &'static str {
    "add message to pool"
}
