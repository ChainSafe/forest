# Entry points

Roots of untrusted input, one `## <surface>` per input surface. `precheck.sh` parses this file: each bullet starts with a repo path in backticks, directories end with `/`, and a changed file under a listed path marks that root as touched in `roots.txt`. The reachability walk ends at one of these roots.

## p2p

- `src/libp2p/service.rs` swarm event loop: gossip, request-response, and peer events from every connected peer
- `src/libp2p/behaviour.rs` gossipsub configuration and subscription filter limits
- `src/libp2p/gossip_params.rs` gossipsub mesh parameters and peer scoring
- `src/libp2p/discovery.rs` Kademlia records, identify address lists, `mDNS`
- `src/libp2p/peer_manager.rs` state keyed by peer ID
- `src/libp2p/rpc/` request-response codec and decoder
- `src/libp2p/chain_exchange/` chain exchange requests served (provider) and responses decoded
- `src/libp2p/hello/` hello codec and messages
- `src/libp2p_bitswap/` bitswap codec, message handling, request manager
- `src/chain_sync/chain_follower.rs` gossip blocks and messages, hello-triggered fetches, mpool add from gossip
- `src/chain_sync/network_context.rs` chain exchange requests sent and their responses
- `src/chain_sync/validation.rs` gossip block validation
- `src/chain_sync/tipset_syncer.rs` tipset and full block validation
- `src/chain_sync/bad_block_cache.rs` rejected blocks cached by CID
- `src/fil_cns/` consensus validation and weight
- `src/blocks/` block header, ticket, election and VRF proof decoding
- `src/message_pool/msgpool/` message add, pending sets, selection, republish

## RPC

- `src/rpc/mod.rs` `for_each_rpc_method!`, server limits (body size, batch length, connections)
- `src/rpc/reflect/` `PERMISSION` per method, parameter parsing
- `src/rpc/methods/` every method handler
- `src/rpc/auth_layer.rs` token and permission checks
- `src/rpc/channel.rs` WebSocket subscriptions
- `src/rpc/parallel_batch_layer.rs` batch execution and response size cap
- `src/rpc/filter_layer.rs` method filter list
- `src/lotus_json/` parameter decoding (big integers, CIDs, bytes)
- `src/eth/` eth transaction RLP decoding from `eth_sendRawTransaction`
- `src/health/` health check HTTP endpoints
- `src/metrics/` metrics HTTP endpoint

## Files and FFI

- `src/db/car/` CAR and forest-CAR snapshot loading, zstd frames, indexes
- `src/utils/db/car_stream.rs` streamed CAR header and block parsing
- `src/daemon/db_util.rs` snapshot import
- `src/cli_shared/snapshot.rs` snapshot download
- `src/utils/net/download_file.rs` downloads to disk
- `src/f3/snapshot.rs` F3 snapshot data inside CAR files
- `src/networks/actors_bundle.rs` actor bundle download and loading
- `src/daemon/bundle.rs` actor bundle loading at startup
- `src/utils/proofs_api/` proof parameter fetch and verification

## F3 sidecar

- `f3-sidecar/` Go component in-process: own go-libp2p host and gossipsub, holds the node JWT, calls Forest RPC
- `src/f3/` FFI boundary and sidecar start and stop
- `src/rpc/methods/f3.rs` RPC methods the sidecar calls

## drand

- `src/beacon/` beacon entry fetch, round ranges, signature verification
