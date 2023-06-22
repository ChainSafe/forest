[client]
encrypt_keystore = false
data_dir = "/forest_data"

[network]
kademlia = false
target_peer_count = 1

# Note that this has to come last. The actual TOML file will have
# the chain name appended.
[chain.network]
type = "devnet"
