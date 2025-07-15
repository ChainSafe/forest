---
title: Metrics
---

Prometheus metrics are exposed on localhost's port `6116` by default, under `/metrics`. They are enabled by default and can be disabled with the `--no-metrics` flag. The metrics endpoint can be modified with the `--metrics-address` flag.

```bash
curl localhost:6116/metrics
```

Sample output:

```console
# HELP lru_cache_miss Stats of lru cache miss.
# TYPE lru_cache_miss counter
lru_cache_miss_total{kind="tipset"} 7199
lru_cache_miss_total{kind="sm_tipset"} 181
# HELP lru_cache_hit Stats of lru cache hit.
# TYPE lru_cache_hit counter
lru_cache_hit_total{kind="sm_tipset"} 913
lru_cache_hit_total{kind="tipset"} 971846
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
