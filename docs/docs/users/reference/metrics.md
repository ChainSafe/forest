---
title: Metrics
---


| Metric | Type | Unit | Description |
|--------|------|------|-------------|
| `tipset_processing_time` | Histogram | Seconds | Duration of routine which processes Tipsets to include them in the store |
| `block_validation_time` | Histogram | Seconds | Duration of routine which validate blocks with no cache hit |
| `libp2p_message_total` | Counter | Count | Total number of libp2p messages by type |
| `invalid_tipset_total` | Counter | Count | Total number of invalid tipsets received over gossipsub |
| `head_epoch` | Gauge | Epoch | Latest epoch synchronized to the node |
| `lru_cache_hit` | Counter | Count | Stats of lru cache hit. Indexed by `kind` |
| `lru_cache_miss` | Counter | Count | Stats of lru cache miss. Indexed by `kind` |
| `rpc_method_failure` | Counter | Count | Number of failed RPC calls. Indexed by `method` |
| `rpc_processing_time` | Histogram | Seconds | Duration of RPC method processing. Indexed by `method` |
| `peer_failure_total` | Counter | Count | Total number of failed peer requests |
| `full_peers` | Gauge | Count | Number of healthy peers recognized by the node |
| `bad_peers` | Gauge | Count | Number of bad peers recognized by the node |
| `expected_network_height` | Gauge | Count | The expected network height based on the current time and the genesis block time |
| `forest_db_size` | Gauge | Bytes | Size of Forest database in bytes |
| `bitswap_message_count` | Counter | Count | Number of bitswap messages. Indexed by `type` |
| `bitswap_container_capacities` | Gauge | Count | Capacity for each bitswap container. Indexed by `type` |
| `bitswap_get_block_time` | Histogram | Seconds | Duration of get_block |
| `mpool_message_total` | Gauge | Count | Total number of messages in the message pool |
| `build_info` | Gauge | N/A | Semantic version of the forest binary. Indexed by `version`. |
| `process_start_time_seconds` | Gauge | Seconds | Time that the process started (in seconds since the UNIX epoch) |
| `process_uptime_seconds` | Counter | Seconds | Total time since the process started |
| `libp2p_bandwidth_bytes_total` | Counter | Bytes | Bandwidth usage by direction and transport protocols. Indexed by `protocols` and `direction` |

<details>
  <summary>Example `bitswap_message_count_total` output</summary>
```
# HELP bitswap_message_count Number of bitswap messages.
# TYPE bitswap_message_count counter
bitswap_message_count_total{type="inbound_request_have"} 1
bitswap_message_count_total{type="inbound_stream_count"} 2
```
</details>

<details>
  <summary>Example `bitswap_container_capacities` output</summary>
```
# HELP bitswap_container_capacities Capacity for each bitswap container.
# TYPE bitswap_container_capacities gauge
bitswap_container_capacities{type="peer_container_capacity"} 27
```
</details>


<details>
  <summary>Example `bitswap_get_block_time` output</summary>
```
# HELP bitswap_get_block_time Duration of get_block.
# TYPE bitswap_get_block_time histogram
bitswap_get_block_time_sum 0.0
bitswap_get_block_time_count 0
bitswap_get_block_time_bucket{le="0.1"} 0
bitswap_get_block_time_bucket{le="0.5"} 0
bitswap_get_block_time_bucket{le="0.75"} 0
bitswap_get_block_time_bucket{le="1.0"} 0
bitswap_get_block_time_bucket{le="1.5"} 0
bitswap_get_block_time_bucket{le="2.0"} 0
bitswap_get_block_time_bucket{le="3.0"} 0
bitswap_get_block_time_bucket{le="4.0"} 0
bitswap_get_block_time_bucket{le="5.0"} 0
bitswap_get_block_time_bucket{le="6.0"} 0
bitswap_get_block_time_bucket{le="7.0"} 0
bitswap_get_block_time_bucket{le="8.0"} 0
bitswap_get_block_time_bucket{le="9.0"} 0
bitswap_get_block_time_bucket{le="10.0"} 0
bitswap_get_block_time_bucket{le="+Inf"} 0
```
</details>

<details>
  <summary>Example `lru_cache_miss` output</summary>
```
# HELP lru_cache_miss Stats of lru cache miss.
# TYPE lru_cache_miss counter
lru_cache_miss_total{kind="sm_tipset"} 37
lru_cache_miss_total{kind="tipset"} 7046
```
</details>

<details>
  <summary>Example `rpc_processing_time` output</summary>
```
# HELP rpc_processing_time Duration of RPC method call in milliseconds.
# TYPE rpc_processing_time histogram
rpc_processing_time_sum{method="F3.GetHead"} 0.7877869999999999
rpc_processing_time_count{method="F3.GetHead"} 30
rpc_processing_time_bucket{le="0.1",method="F3.GetHead"} 29
rpc_processing_time_bucket{le="1.0",method="F3.GetHead"} 30
rpc_processing_time_bucket{le="10.0",method="F3.GetHead"} 30
rpc_processing_time_bucket{le="100.0",method="F3.GetHead"} 30
rpc_processing_time_bucket{le="1000.0",method="F3.GetHead"} 30
rpc_processing_time_bucket{le="+Inf",method="F3.GetHead"} 30
rpc_processing_time_sum{method="F3.GetPowerTable"} 406.521251
rpc_processing_time_count{method="F3.GetPowerTable"} 7
rpc_processing_time_bucket{le="0.1",method="F3.GetPowerTable"} 0
rpc_processing_time_bucket{le="1.0",method="F3.GetPowerTable"} 0
rpc_processing_time_bucket{le="10.0",method="F3.GetPowerTable"} 4
rpc_processing_time_bucket{le="100.0",method="F3.GetPowerTable"} 6
rpc_processing_time_bucket{le="1000.0",method="F3.GetPowerTable"} 7
rpc_processing_time_bucket{le="+Inf",method="F3.GetPowerTable"} 7
rpc_processing_time_sum{method="Filecoin.NetAddrsListen"} 434.141625
rpc_processing_time_count{method="Filecoin.NetAddrsListen"} 1
rpc_processing_time_bucket{le="0.1",method="Filecoin.NetAddrsListen"} 0
rpc_processing_time_bucket{le="1.0",method="Filecoin.NetAddrsListen"} 0
rpc_processing_time_bucket{le="10.0",method="Filecoin.NetAddrsListen"} 0
rpc_processing_time_bucket{le="100.0",method="Filecoin.NetAddrsListen"} 0
rpc_processing_time_bucket{le="1000.0",method="Filecoin.NetAddrsListen"} 1
rpc_processing_time_bucket{le="+Inf",method="Filecoin.NetAddrsListen"} 1
rpc_processing_time_sum{method="F3.GetParticipatingMinerIDs"} 51.30074899999999
rpc_processing_time_count{method="F3.GetParticipatingMinerIDs"} 58
rpc_processing_time_bucket{le="0.1",method="F3.GetParticipatingMinerIDs"} 0
rpc_processing_time_bucket{le="1.0",method="F3.GetParticipatingMinerIDs"} 39
rpc_processing_time_bucket{le="10.0",method="F3.GetParticipatingMinerIDs"} 58
rpc_processing_time_bucket{le="100.0",method="F3.GetParticipatingMinerIDs"} 58
rpc_processing_time_bucket{le="1000.0",method="F3.GetParticipatingMinerIDs"} 58
rpc_processing_time_bucket{le="+Inf",method="F3.GetParticipatingMinerIDs"} 58
rpc_processing_time_sum{method="F3.GetTipset"} 0.282751
rpc_processing_time_count{method="F3.GetTipset"} 8
rpc_processing_time_bucket{le="0.1",method="F3.GetTipset"} 8
rpc_processing_time_bucket{le="1.0",method="F3.GetTipset"} 8
rpc_processing_time_bucket{le="10.0",method="F3.GetTipset"} 8
rpc_processing_time_bucket{le="100.0",method="F3.GetTipset"} 8
rpc_processing_time_bucket{le="1000.0",method="F3.GetTipset"} 8
rpc_processing_time_bucket{le="+Inf",method="F3.GetTipset"} 8
rpc_processing_time_sum{method="F3.Finalize"} 12.134668999999999
rpc_processing_time_count{method="F3.Finalize"} 26
rpc_processing_time_bucket{le="0.1",method="F3.Finalize"} 1
rpc_processing_time_bucket{le="1.0",method="F3.Finalize"} 23
rpc_processing_time_bucket{le="10.0",method="F3.Finalize"} 26
rpc_processing_time_bucket{le="100.0",method="F3.Finalize"} 26
rpc_processing_time_bucket{le="1000.0",method="F3.Finalize"} 26
rpc_processing_time_bucket{le="+Inf",method="F3.Finalize"} 26
rpc_processing_time_sum{method="F3.GetParent"} 0.306957
rpc_processing_time_count{method="F3.GetParent"} 10
rpc_processing_time_bucket{le="0.1",method="F3.GetParent"} 10
rpc_processing_time_bucket{le="1.0",method="F3.GetParent"} 10
rpc_processing_time_bucket{le="10.0",method="F3.GetParent"} 10
rpc_processing_time_bucket{le="100.0",method="F3.GetParent"} 10
rpc_processing_time_bucket{le="1000.0",method="F3.GetParent"} 10
rpc_processing_time_bucket{le="+Inf",method="F3.GetParent"} 10
rpc_processing_time_sum{method="F3.ProtectPeer"} 0.164208
rpc_processing_time_count{method="F3.ProtectPeer"} 1
rpc_processing_time_bucket{le="0.1",method="F3.ProtectPeer"} 0
rpc_processing_time_bucket{le="1.0",method="F3.ProtectPeer"} 1
rpc_processing_time_bucket{le="10.0",method="F3.ProtectPeer"} 1
rpc_processing_time_bucket{le="100.0",method="F3.ProtectPeer"} 1
rpc_processing_time_bucket{le="1000.0",method="F3.ProtectPeer"} 1
rpc_processing_time_bucket{le="+Inf",method="F3.ProtectPeer"} 1
rpc_processing_time_sum{method="Filecoin.StateNetworkName"} 4.00525
rpc_processing_time_count{method="Filecoin.StateNetworkName"} 1
rpc_processing_time_bucket{le="0.1",method="Filecoin.StateNetworkName"} 0
rpc_processing_time_bucket{le="1.0",method="Filecoin.StateNetworkName"} 0
rpc_processing_time_bucket{le="10.0",method="Filecoin.StateNetworkName"} 1
rpc_processing_time_bucket{le="100.0",method="Filecoin.StateNetworkName"} 1
rpc_processing_time_bucket{le="1000.0",method="Filecoin.StateNetworkName"} 1
rpc_processing_time_bucket{le="+Inf",method="Filecoin.StateNetworkName"} 1
rpc_processing_time_sum{method="Filecoin.Version"} 0.031375
rpc_processing_time_count{method="Filecoin.Version"} 1
rpc_processing_time_bucket{le="0.1",method="Filecoin.Version"} 1
rpc_processing_time_bucket{le="1.0",method="Filecoin.Version"} 1
rpc_processing_time_bucket{le="10.0",method="Filecoin.Version"} 1
rpc_processing_time_bucket{le="100.0",method="Filecoin.Version"} 1
rpc_processing_time_bucket{le="1000.0",method="Filecoin.Version"} 1
rpc_processing_time_bucket{le="+Inf",method="Filecoin.Version"} 1
```
</details>

<details>
  <summary>Example `libp2p_messsage_total` output</summary>
```
# HELP libp2p_messsage_total Total number of libp2p messages by type.
# TYPE libp2p_messsage_total counter
libp2p_messsage_total_total{libp2p_message_kind="chain_exchange_request_in"} 2
libp2p_messsage_total_total{libp2p_message_kind="hello_request_in"} 33
libp2p_messsage_total_total{libp2p_message_kind="chain_exchange_response_in"} 62
libp2p_messsage_total_total{libp2p_message_kind="pubsub_message_message"} 1
libp2p_messsage_total_total{libp2p_message_kind="peer_connected"} 29
libp2p_messsage_total_total{libp2p_message_kind="peer_disconnected"} 3
libp2p_messsage_total_total{libp2p_message_kind="hello_response_out"} 33
libp2p_messsage_total_total{libp2p_message_kind="chain_exchange_request_out"} 64
libp2p_messsage_total_total{libp2p_message_kind="pubsub_message_block"} 12
libp2p_messsage_total_total{libp2p_message_kind="hello_request_out"} 29
libp2p_messsage_total_total{libp2p_message_kind="chain_exchange_response_out"} 2
libp2p_messsage_total_total{libp2p_message_kind="hello_response_in"} 27
```
</details>

<details>
  <summary>Example `network_container_capacities` output</summary>
```
# HELP network_container_capacities Capacity for each container.
# TYPE network_container_capacities gauge
network_container_capacities{kind="hello_request_table"} 14
network_container_capacities{kind="cx_request_table"} 7
```
</details>

<details>
  <summary>Example `peer_failure_total` output</summary>
```
# HELP peer_failure_total Total number of failed peer requests.
# TYPE peer_failure_total counter
peer_failure_total_total 2
```
</details>

<details>
  <summary>Example `tipset_processing_time` output</summary>
```
# HELP tipset_processing_time Duration of routine which processes Tipsets to include them in the store.
# TYPE tipset_processing_time histogram
tipset_processing_time_sum 7.742167081000002
tipset_processing_time_count 45
tipset_processing_time_bucket{le="0.005"} 8
tipset_processing_time_bucket{le="0.01"} 9
tipset_processing_time_bucket{le="0.025"} 37
tipset_processing_time_bucket{le="0.05"} 40
tipset_processing_time_bucket{le="0.1"} 43
tipset_processing_time_bucket{le="0.25"} 43
tipset_processing_time_bucket{le="0.5"} 43
tipset_processing_time_bucket{le="1.0"} 43
tipset_processing_time_bucket{le="2.5"} 43
tipset_processing_time_bucket{le="5.0"} 45
tipset_processing_time_bucket{le="10.0"} 45
tipset_processing_time_bucket{le="+Inf"} 45
```
</details>

<details>
  <summary>Example `block_validation_time` output</summary>
```
# HELP block_validation_time Duration of routine which validate blocks with no cache hit.
# TYPE block_validation_time histogram
block_validation_time_sum 19.254469710000014
block_validation_time_count 90
block_validation_time_bucket{le="0.005"} 2
block_validation_time_bucket{le="0.01"} 3
block_validation_time_bucket{le="0.025"} 72
block_validation_time_bucket{le="0.05"} 78
block_validation_time_bucket{le="0.1"} 85
block_validation_time_bucket{le="0.25"} 85
block_validation_time_bucket{le="0.5"} 85
block_validation_time_bucket{le="1.0"} 85
block_validation_time_bucket{le="2.5"} 85
block_validation_time_bucket{le="5.0"} 90
block_validation_time_bucket{le="10.0"} 90
block_validation_time_bucket{le="+Inf"} 90
```
</details>

<details>
  <summary>Example `full_peers` output</summary>
```
# HELP full_peers Number of healthy peers recognized by the node.
# TYPE full_peers gauge
full_peers 25
```
</details>

<details>
  <summary>Example `bad_peers` output</summary>
```
# HELP bad_peers Number of bad peers recognized by the node.
# TYPE bad_peers gauge
bad_peers 1
```
</details>

<details>
  <summary>Example `head_epoch` output</summary>
```
# HELP head_epoch Latest epoch synchronized to the node.
# TYPE head_epoch gauge
head_epoch 2519530
```
</details>

<details>
  <summary>Example `expected_network_height` output</summary>
```
# HELP expected_network_height The expected network height based on the current time and the genesis block time
# TYPE expected_network_height gauge
expected_network_height 2519530
```
</details>

<details>
  <summary>Example `build_info` output</summary>
```
# HELP build_info semantic version of the forest binary
# TYPE build_info gauge
build_info{version="0.25.0+git.9771eec46d3"} 1
```
</details>

<details>
  <summary>Example `forest_db_size` output</summary>
```
# HELP forest_db_size Size of Forest database in bytes
# TYPE forest_db_size gauge
forest_db_size 5941414576
```
</details>

<details>
  <summary>Example `libp2p_bandwidth_bytes` output</summary>
```
# HELP libp2p_bandwidth_bytes Bandwidth usage by direction and transport protocols.
# TYPE libp2p_bandwidth_bytes counter
# UNIT libp2p_bandwidth_bytes bytes
libp2p_bandwidth_bytes_total{protocols="/ip6/tcp/p2p",direction="Inbound"} 0
libp2p_bandwidth_bytes_total{protocols="/ip4/tcp",direction="Inbound"} 9413
libp2p_bandwidth_bytes_total{protocols="/ip4/tcp",direction="Outbound"} 29471
libp2p_bandwidth_bytes_total{protocols="/ip4/udp/quic-v1/webtransport/certhash/certhash/p2p",direction="Outbound"} 0
libp2p_bandwidth_bytes_total{protocols="/dns/tcp/p2p",direction="Inbound"} 378094
libp2p_bandwidth_bytes_total{protocols="/ip6/udp/quic-v1/p2p",direction="Inbound"} 0
libp2p_bandwidth_bytes_total{protocols="/ip6/udp/quic-v1/webtransport/certhash/certhash/p2p",direction="Inbound"} 0
libp2p_bandwidth_bytes_total{protocols="/ip4/udp/quic-v1/webtransport/certhash/certhash/p2p",direction="Inbound"} 0
libp2p_bandwidth_bytes_total{protocols="/ip6/udp/quic-v1/webtransport/certhash/certhash/p2p",direction="Outbound"} 0
libp2p_bandwidth_bytes_total{protocols="/ip4/udp/quic-v1/p2p",direction="Inbound"} 491457
libp2p_bandwidth_bytes_total{protocols="/ip6/tcp/p2p",direction="Outbound"} 0
libp2p_bandwidth_bytes_total{protocols="/ip4/tcp/p2p",direction="Outbound"} 56789
libp2p_bandwidth_bytes_total{protocols="/ip4/tcp/p2p",direction="Inbound"} 627818
libp2p_bandwidth_bytes_total{protocols="/ip4/udp/quic-v1/p2p",direction="Outbound"} 86350
libp2p_bandwidth_bytes_total{protocols="/dns/tcp/p2p",direction="Outbound"} 18720
libp2p_bandwidth_bytes_total{protocols="/ip6/udp/quic-v1/p2p",direction="Outbound"} 0
```
</details>

<details>
  <summary>Example `process_start_time_seconds` output</summary>
```
# HELP process_start_time_seconds Time that the process started (in seconds since the UNIX epoch).
# TYPE process_start_time_seconds gauge
# UNIT process_start_time_seconds seconds
process_start_time_seconds 1742912218.100066
```
</details>

<details>
  <summary>Example `process_uptime_seconds` output</summary>
```
# HELP process_uptime_seconds Total time since the process started (in seconds)
# TYPE process_uptime_seconds counter
# UNIT process_uptime_seconds seconds
process_uptime_seconds_total 84.24605
```
</details>
