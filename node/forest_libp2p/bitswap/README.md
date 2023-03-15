# forest_libp2p_bitswap

Another libp2p
bitswap([SPEC](https://github.com/ipfs/specs/blob/main/BITSWAP.md))
implementation in Rust.

## Features

- Compatible with [`go-bitswap`](https://github.com/ipfs/go-bitswap)
- Optional request manager
- Prometheus metrics
- Multiple async task API support, `async-std` and `tokio`(optional behind
  feature `tokio`)
- Compiles into WebAssembly and works in browser.
  (`examples/bitswap-in-browser`)

## Feature flags

- `tokio`, disabled by default. Use task API(s) from `tokio` instead of
  `async-std` inside this crate.

Note: since `async-std` task API(s) are compatible with `tokio` runtime, it
still works fine with `tokio` runtime when this feature is disabled. But it
won't work with `async-std` runtime if this feature is enabled.

## Usage

Basic usage of `BitswapBehaviour`, for writing swarm event flow, sending or
receiving a request or a response, checkout `tests/go_compat.rs`. Note that a
request manager is needed for a real-world application.

```rust
use forest_libp2p_bitswap::BitswapBehaviour;

// Use default protocol ID(s), same with `go-bitswap` defaults
let behaviour = BitswapBehaviour::default();

// Use custom protocol ID(s)
let behaviour = BitswapBehaviour::new(&[b"/test/ipfs/bitswap/1.2.0"], Default::default());
```

To use the builtin request manager that is optimized for filecoin network, a
data store that implements `BitswapStoreRead` and `BitswapStoreReadWrite` is
required. For hooking request manager in swarm event flow, requesting a block
via request manager API, checkout `tests/request_manager.rs`.

```rust
use forest_libp2p_bitswap::BitswapBehaviour;

let behaviour = BitswapBehaviour::default();
// Gets the associated request manager from the bitswap behaviour
// Note: The response is of type Arc<BitswapRequestManager> so that
// you can easily clone it, store it or send it around.
let bitswap_request_manager = behaviour.request_manager();
```
