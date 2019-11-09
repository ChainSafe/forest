use message::SignedMessage;

trait PoolValidator {
	fn Validate(&self, msg: SignedMessage);
}

// MessagePoolConfig holds all configuration options related to nodes message pool (mpool).
pub struct MessagePoolConfig {
	// MaxPoolSize is the maximum number of pending messages will will allow in the message pool at any time
	MaxPoolSize: u64, 
	// MaxNonceGap is the maximum nonce of a message past the last received on chain
	MaxNonceGap: u64,
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
	addedAt: u64,
}

fn newDefaultMessagePoolConfig() -> MessagePoolConfig {
    return MessagePoolConfig { MaxPoolSize: 10000, MaxNonceGap: 100 };
}

// NewPool constructs a new Pool.
fn NewPool() -> &'static str {
	return "Pool{ cfg: cfg, validator: validator}"
}

struct addressNonce {
	//addr  address
	nonce: u64
}

fn newAddressNonce(msg: SignedMessage) -> &'static str {
	return "return addressNonce"
}

// Add adds a message to the pool, tagged with the block height at which it was received.
// Does nothing if the message is already in the pool.
fn add(msg: SignedMessage, height: u64) -> &'static str {
	return "add message to pool"
}