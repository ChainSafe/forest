[client]
encrypt_keystore = false
data_dir = "/forest_data"

[network]
kademlia = false
# The connection limits are derived from this: at most `4 * target_peer_count`
# established inbound connections. A count of 1 leaves room for four, which the
# F3 sidecar alone fills up - any further peer then gets its connection denied
# right after the handshake.
target_peer_count = 10
# A fixed port, so that other containers have a stable address to dial. The
# placeholder is substituted with FOREST_P2P_PORT by `docker-compose.yml`.
listening_multiaddrs = ["/ip4/0.0.0.0/tcp/__FOREST_P2P_PORT__"]

# Note that this has to come last. The actual TOML file will have
# the chain name appended.
[chain]
type = "devnet"
