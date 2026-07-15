---
sidebar_position: 2
title: RPC Performance Comparison
---

# RPC Performance: Forest vs Lotus

The following measurements were taken while a Forest node and a Lotus node handled a similar stream of real RPC provider traffic during the same window. Because both nodes served an almost identical request stream, the differences below are attributable to the nodes rather than to uneven load. The "Comparable load" section documents the checks used to support that assumption.

## Environment

The measurements cover Lotus `1.36.1-rc1` and Forest `0.33.8`.

![Build info for the compared Forest and Lotus nodes](/img/reports/rpc_comparison/build-info.png)

## RPC latency

Average wall time per HTTP request was approximately 5 ms for Forest and 40-80 ms for Lotus over the measurement window. The Forest trace stayed roughly constant, while the Lotus trace varied as the request mix changed.

![Average wall time per HTTP request](/img/reports/rpc_comparison/wall-time-per-http-request.png)

### Per-flow latency

At the median (P50) the two nodes were close: approximately 6-7 ms for Forest and 8-12 ms for Lotus. The difference was larger in the tail. At P95, Forest stayed near 25 ms while Lotus stayed around 90 ms and reached approximately 450-470 ms under load. At P99, Forest held around 150 ms while Lotus ranged from approximately 500 ms to 1.6 s.

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

The request rates and batch sizes reaching each node were close, which supports comparing the two nodes directly. HTTP request count averaged 44.9 `req/s` for Forest and 45.2 `req/s` for Lotus; batch-aware counts were 25.4 vs 25.9 `req/s`; and average batch size was approximately 1.28 vs 1.29. Both nodes therefore processed a comparable amount of work.

![Request count (HTTP only)](/img/reports/rpc_comparison/request-count-http.png)

![Request count (batch-aware)](/img/reports/rpc_comparison/request-count-batch-aware.png)

![Average batch size](/img/reports/rpc_comparison/average-batch-size.png)

Concurrency followed from the latency difference: Forest kept fewer requests in flight at any instant (mean 0.39, peak 5) than Lotus (mean 1.90, peak 16).

![In-flight requests](/img/reports/rpc_comparison/in-flight-requests.png)

## Resource usage

While serving this workload, Forest used approximately 10% CPU and Lotus used approximately 40-80%, trending upward over the session. Forest's CPU trace was also less variable.

![CPU usage](/img/reports/rpc_comparison/cpu.png)

Memory usage was approximately 10% for Forest and approximately 25-30% for Lotus.

![Memory usage](/img/reports/rpc_comparison/memory.png)

:::note
The absolute memory figures are higher than they appear here because of OS page caches, but the relative proportions between the two nodes still hold.
:::

## Related

- [Snapshot Generation Comparison](./snapshot_comparison.md)
- [Forest vs Lotus feature comparison](./index.md)
- [RPC performance Grafana dashboard](https://monitoring.chain.love/public-dashboards/229eaf7ce0224655abe26c288aab914b?orgId=1&refresh=5m&kiosk)
