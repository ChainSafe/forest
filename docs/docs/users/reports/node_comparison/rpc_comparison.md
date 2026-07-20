---
sidebar_position: 2
title: RPC Performance Comparison
---

# RPC Performance: Forest vs Lotus

The following measurements were taken while a Forest node and a Lotus node handled a similar stream of real RPC provider traffic during the same time window. Because both nodes served an almost identical request stream, the differences below are attributable to the nodes themselves rather than to uneven load. The "Comparable load" section documents the checks used to support that assumption.

## Environment

The measurements cover the following node builds:

| Node   | Version      | Commit        |
| ------ | ------------ | ------------- |
| Forest | `0.34.1`     | `git.0ba362e` |
| Lotus  | `1.36.1-rc1` | `git.45eade8` |

## RPC latency

Average wall time per HTTP request was approximately 10-15 ms for Forest and 30-50 ms for Lotus over most of the measurement window. Both traces rose toward the end as load increased, with Forest reaching about 30 ms and Lotus about 48 ms.

![Average wall time per HTTP request](/img/reports/rpc_comparison/wall-time-per-http-request.png)

### Per-flow latency

At the median (P50), the two nodes were close: approximately 7 ms for Forest and 8 ms for Lotus. The difference was larger in the tail. At P95, Forest stayed near 50 ms and rose to roughly 115 ms under load, while Lotus stayed around 70-85 ms and reached roughly 185 ms. At P99, Forest held around 200 ms and rose to about 350 ms, while Lotus ranged from approximately 240 ms to a peak near 600 ms.

<p align="center">
  <img
    src="/img/reports/rpc_comparison/p50-per-flow-latency.png"
    alt="P50 per-flow latency"
    style={{ maxWidth: '680px', width: '100%' }}
  />
</p>

<p align="center">
  <img
    src="/img/reports/rpc_comparison/p95-per-flow-latency.png"
    alt="P95 per-flow latency"
    style={{ maxWidth: '680px', width: '100%' }}
  />
</p>

<p align="center">
  <img
    src="/img/reports/rpc_comparison/p99-per-flow-latency.png"
    alt="P99 per-flow latency"
    style={{ maxWidth: '680px', width: '100%' }}
  />
</p>

### Amortized latency

The amortized view accounts for batched requests. It shows the same pattern as the per-flow view: comparable values at P50 and a larger difference at P95, P99, and on average.

<p align="center">
  <img
    src="/img/reports/rpc_comparison/p50-amortized-latency.png"
    alt="P50 amortized latency"
    style={{ maxWidth: '680px', width: '100%' }}
  />
</p>

<p align="center">
  <img
    src="/img/reports/rpc_comparison/p95-amortized-latency.png"
    alt="P95 amortized latency"
    style={{ maxWidth: '680px', width: '100%' }}
  />
</p>

<p align="center">
  <img
    src="/img/reports/rpc_comparison/p99-amortized-latency.png"
    alt="P99 amortized latency"
    style={{ maxWidth: '680px', width: '100%' }}
  />
</p>

![Average amortized latency](/img/reports/rpc_comparison/average-amortized-latency.png)

### Comparable load

The request rates reaching each node were close, which supports comparing the two nodes directly. HTTP request count averaged 45.2 `req/s` for Forest and 45.0 `req/s` for Lotus, and batch-aware counts were 20.4 vs 20.3 `req/s`. Both nodes therefore received a comparable volume of traffic.

![Request count (HTTP only)](/img/reports/rpc_comparison/request-count-http.png)

![Request count (batch-aware)](/img/reports/rpc_comparison/request-count-batch-aware.png)

Concurrency followed from the latency difference: Forest kept fewer requests in flight at any instant (mean 0.43, peak 4) than Lotus (mean 1.45, peak 8).

![In-flight requests](/img/reports/rpc_comparison/in-flight-requests.png)

## Resource usage

While serving this workload, Forest used approximately 10-15% CPU and Lotus used approximately 45-55%, briefly spiking to about 80% during the higher-load period. Forest's CPU trace was also less variable.

![CPU usage](/img/reports/rpc_comparison/cpu.png)

Memory usage was approximately 8-10% for Forest and approximately 25-30% for Lotus, with Lotus briefly reaching about 40% during the higher-load period.

![Memory usage](/img/reports/rpc_comparison/memory.png)

:::note
The absolute memory figures are higher than they appear here because of OS page caches, but the relative proportions between the two nodes still hold.
:::

## Related

- [Snapshot Generation Comparison](./snapshot_comparison.md)
- [Forest vs Lotus feature comparison](./index.md)
- [RPC performance Grafana dashboard](https://monitoring.chain.love/public-dashboards/229eaf7ce0224655abe26c288aab914b?orgId=1&refresh=5m&kiosk)
