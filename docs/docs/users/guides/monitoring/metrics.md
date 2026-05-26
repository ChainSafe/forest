---
title: Metrics
---

Prometheus metrics are exposed on localhost's port `6116` by default, under `/metrics`. They are enabled by default and can be disabled with the `--no-metrics` flag. The metrics endpoint can be modified with the `--metrics-address` flag.

```bash
curl localhost:6116/metrics
```

Sample output:

```console
# HELP cache_tipset_size_bytes Size of cache tipset in bytes
# TYPE cache_tipset_size_bytes gauge
# UNIT cache_tipset_size_bytes bytes
cache_tipset_size_bytes 10877000
# HELP cache_tipset_len Length of cache tipset
# TYPE cache_tipset_len gauge
cache_tipset_len 2880
# HELP cache_tipset_cap Capacity of cache tipset
# TYPE cache_tipset_cap gauge
cache_tipset_cap 2880
# HELP cache_tipset_hits Cache hits of tipset
# TYPE cache_tipset_hits gauge
cache_tipset_hits 19795
# HELP cache_tipset_misses Cache misses of tipset
# TYPE cache_tipset_misses gauge
cache_tipset_misses 39026
...
```

The metrics include:

- networking metrics (e.g., number of peers, number of blocks received),
- database metrics (e.g., database size),
- RPC metrics (e.g., number of requests, response times),
- internal metrics (e.g., cache sizes, number of tasks).

Those can be used to monitor the node's health and create alerts. A sample monitoring stack is available in the [monitored-stack](https://github.com/ChainSafe/forest/tree/main/monitored-stack) directory in the Forest repository. It includes the entire Docker Compose setup to run Forest with monitoring locally. See the instructions in the [README](https://github.com/ChainSafe/forest/blob/main/monitored-stack/README.md) file.

:::tip
Because of the high cardinality of some of the metrics, a high retention period, together with high sampling rates, can lead to a large amount of data being stored. Make sure to adjust the retention period and sampling rates to your needs.
:::

:::info
If you need additional metrics, contact the Forest team. We can help you add new metrics to the node or expose additional information.
:::
