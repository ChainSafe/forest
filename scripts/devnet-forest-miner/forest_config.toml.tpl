[client]
encrypt_keystore = false
data_dir = "/forest_data"

[network]
# The devnet chain config ships no bootstrap peers, so `docker-compose.yml`
# substitutes the placeholder with the Lotus validating node's address.
bootstrap_peers = ["__LOTUS_MULTIADDR__"]
