[client]
encrypt_keystore = false
data_dir = "/forest_data"

[network]
# The connection limits are derived from this: at most `4 * target_peer_count`
# established inbound connections. A count of 1 leaves room for four, which the
# F3 sidecar alone fills up - any further peer then gets its connection denied
# right after the handshake.
target_peer_count = 10
# The devnet chain config ships no bootstrap peers, so `docker-compose.yml`
# substitutes the placeholder with the Lotus node's address.
bootstrap_peers = ["__LOTUS_MULTIADDR__"]
