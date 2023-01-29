# forest_libp2p_bitswap

Another libp2p bitswap([SPEC](https://github.com/ipfs/specs/blob/main/BITSWAP.md)) implementation in rust.

## Features

- Compatible with [`go-bitswap`](https://github.com/ipfs/go-bitswap)
- Optional request manager
- Prometheus metrics
- Multiple async task API support, `async-std` and `tokio`(optional behind feature `tokio`)

## Feature flags

- `tokio`, disabled by default. Use task API(s) from `tokio` instead of `async-std` inside this crate.

Note: since `async-std` task API(s) are compatible with `tokio` runtime, so it still works fine with `tokio` runtime when this feature is disabled. But it won't work with `async-std` runtime if this feature is enabled.

## Usage

Basic usage of `BitswapBehaviour`, checkout `tests/go_compat.rs` for details

```rust
// Use default protocol ID(s), same with `go-bitswap` defaults
let behaviour = BitswapBehaviour::default();

// Use custom protocol ID(s)
let behaviour = BitswapBehaviour::new(&[b"/test/ipfs/bitswap/1.2.0"], Default::default());

// Swarm event
match swarm.select_next_some().await {
    SwarmEvent::Behaviour(BitswapBehaviourEvent::Message { peer, message }) => {
        // Custom logic
    }
}

// Send bitswap request
swarm.behaviour_mut().send_request(peer, request);

// Send bitswap response
swarm.behaviour_mut().send_response(peer, response);
```

To use request manager, a data store that implements `BitswapStoreRead` and `BitswapStoreReadWrite` is required. Checkout `tests/request_manager.rs` for details

```rust
let behaviour = BitswapBehaviour::default();
// Gets the accociated request manager from the bitswap behaviour
// Note: The response is of type Arc<BitswapRequestManager> so that
// you can easily clone it, store it or send it around.
let bitswap_request_manager = behaviour.request_manager();
let swarm = ...;

let mut outbound_request_rx_stream = request_manager.outbound_request_rx().stream().fuse();
// Hook libp2p swarm events
loop {
    select! {
        // Hook swarm event
        swarm_event = match swarm.select_next_some() => match swarm_event {
            SwarmEvent::Behaviour(BitswapBehaviourEvent::Message { peer, message }) => {
                let bitswap = &mut swarm.behaviour_mut();
                // `store` implements `BitswapStore`
                if let Err(err) = bitswap.handle_event(store, event) {
                    // log
                }
            }
        },
        // Hook request manager outgoing message
        request_opt = outbound_request_rx_stream.next() => if let Some((peer, request)) = request_opt {
            let bitswap = &mut swarm.behaviour_mut();
            bitswap.send_request(&peer, request);
        },
    }
}

// request a block with `cid`
let (tx, rx) = flume::bounded(1);
// NOTE: `get_block` API does not block
bitswap_request_manager.get_block(store, cid, timeout, Some(tx));
let success = rx.recv()?;
assert_eq!(store.contains(&cid), success);
```
