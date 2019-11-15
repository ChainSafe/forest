use message::SignedMessage;

trait PoolValidator {
    fn validate(&self, msg: SignedMessage);
}

// MessagePoolConfig holds all configuration options related to nodes message pool (mpool).
pub struct MessagePoolConfig {
    // max_pool_size is the maximum number of pending messages will will allow in the message pool at any time
    max_pool_size: u64,
    // max_nonce_gap is the maximum nonce of a message past the last received on chain
    max_nonce_gap: u64,
}

// Pool keeps an unordered, de-duplicated set of Messages and supports removal by CID.
// By 'de-duplicated' we mean that insertion of a message by cid that already
// exists is a nop. We use a Pool to store all messages received by this node
// via network or directly created via user command that have yet to be included
// in a block. Messages are removed as they are processed.
//
// Pool is safe for concurrent access.
pub struct Pool {
    cfg: MessagePoolConfig,
    validator: PoolValidator,
    // pending CID messages
    // addressNonce
}

struct TimedMessage {
    // message
    added_at: u64,
}

fn new_default_message_pool_config() -> MessagePoolConfig {
    return MessagePoolConfig {
        max_pool_size: 10000,
        max_nonce_gap: 100,
    };
}

// new_pool constructs a new Pool.
fn new_pool() -> &'static str {
    return "Pool{ cfg: cfg, validator: validator}";
}

struct addressNonce {
    //addr  address
    nonce: u64,
}

fn new_address_nonce(msg: SignedMessage) -> &'static str {
    return "return addressNonce";
}

// Add adds a message to the pool, tagged with the block height at which it was received.
// Does nothing if the message is already in the pool.
fn add(msg: SignedMessage, height: u64) -> &'static str {
    return "add message to pool";
}
