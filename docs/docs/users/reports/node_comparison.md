---
sidebar_position: 2
title: Node Comparison
---

# Node Comparison: Forest vs Lotus

This report compares Forest against Lotus while both nodes serve the similar real RPC provider traffic. Because the two nodes handled an almost identical request stream during the measurement window, the differences below reflect the nodes themselves rather than uneven load.

## Environment

The comparison covers Lotus `1.36.1-rc1` and Forest `0.33.8`.

![Build info for the compared Forest and Lotus nodes](/img/reports/node_comparison/build-info.png)

## RPC latency

The clearest difference is in response latency. Forest keeps the average wall time per HTTP request near ~5 ms and remarkably flat, while Lotus sits an order of magnitude higher (~40-80 ms) and swings noticeably as load shifts.

![Average wall time per HTTP request](/img/reports/node_comparison/wall-time-per-http-request.png)

### Per-flow latency

At the median (P50), the two nodes are close: Forest hovers around ~6-7 ms versus ~8-12 ms for Lotus. The gap widens sharply in the tail. At P95, Forest stays flat near ~25 ms while Lotus baselines around ~90 ms and spikes toward ~450-470 ms under pressure. At P99, Forest holds around ~150 ms while Lotus ranges from ~500 ms up to ~1.6 s. In practice this means the slow requests that hurt reliability the most are far rarer on Forest.

<p align="center">
  <img
    src="/img/reports/node_comparison/p50-per-flow-latency.png"
    alt="P50 per-flow latency"
    style={{ maxWidth: '680px', width: '100%' }}
  />
</p>

<p align="center">
  <img
    src="/img/reports/node_comparison/p95-per-flow-latency.png"
    alt="P95 per-flow latency"
    style={{ maxWidth: '680px', width: '100%' }}
  />
</p>

<p align="center">
  <img
    src="/img/reports/node_comparison/p99-per-flow-latency.png"
    alt="P99 per-flow latency"
    style={{ maxWidth: '680px', width: '100%' }}
  />
</p>

### Amortized latency

The amortized view, which accounts for batched requests, tells the same story: a small Forest advantage at P50 and a large one at P95, P99, and on average.

<p align="center">
  <img
    src="/img/reports/node_comparison/p50-amortized-latency.png"
    alt="P50 amortized latency"
    style={{ maxWidth: '680px', width: '100%' }}
  />
</p>

<p align="center">
  <img
    src="/img/reports/node_comparison/p95-amortized-latency.png"
    alt="P95 amortized latency"
    style={{ maxWidth: '680px', width: '100%' }}
  />
</p>

<p align="center">
  <img
    src="/img/reports/node_comparison/p99-amortized-latency.png"
    alt="P99 amortized latency"
    style={{ maxWidth: '680px', width: '100%' }}
  />
</p>

![Average amortized latency](/img/reports/node_comparison/average-amortized-latency.png)

### Comparable load

To confirm the comparison is fair, the request rates and batch sizes hitting each node line up almost exactly. HTTP request count averages 44.9 `req/s` for Forest and 45.2 `req/s` for Lotus; batch-aware counts are 25.4 vs 25.9 `req/s`; and average batch size is ~1.28 vs ~1.29. In other words, both nodes did essentially the same amount of work.

![Request count (HTTP only)](/img/reports/node_comparison/request-count-http.png)

![Request count (batch-aware)](/img/reports/node_comparison/request-count-batch-aware.png)

![Average batch size](/img/reports/node_comparison/average-batch-size.png)

The faster turnaround also shows up in concurrency: Forest keeps far fewer requests in flight at any instant (mean 0.39, peak 5) compared to Lotus (mean 1.90, peak 16).

![In-flight requests](/img/reports/node_comparison/in-flight-requests.png)

## Resource usage

These latency gains don't come at the cost of higher resource consumption. Forest runs the same workload on roughly ~10% CPU while Lotus sits around ~40-80% and trends upward as the session goes on. Forest's CPU trace is also visibly smoother, indicating steadier, more predictable load on the machine.

![CPU usage](/img/reports/node_comparison/cpu.png)

Memory follows the same pattern, with Forest around ~10% against Lotus's ~25-30%.

![Memory usage](/img/reports/node_comparison/memory.png)

:::note
The absolute memory figures are higher than they appear here because of OS page caches, but the relative proportions between the two nodes still hold.
:::

## Snapshot export comparison

Snapshot export is another area where the performance work pays off. Forest plays an important role in timely, efficient network snapshot generation: while it is possible to generate a snapshot with Lotus, doing so is significantly slower and more expensive. On a regular machine, Forest completes a basic chain snapshot export dramatically faster than Lotus, and with a fraction of the memory.

<p align="center">
  <img
    src="/img/reports/node_comparison/snapshot-export-comparison.png"
    alt="Snapshot export duration: Forest vs Lotus"
    style={{ maxWidth: '480px', width: '100%' }}
  />
</p>

Forest `0.33.6` finished the export in 34 minutes using 32 GiB of RAM. Lotus `1.36.0` took 450 minutes (about `13x` slower) with 128 GiB, and 232 minutes (about `7x` slower) even with 256 GiB. Forest also needs less than half the disk space for the resulting snapshot. The table below compares a basic snapshot export across implementations.

|        | Required disk space [GiB] | RAM [GiB] | Export duration [minutes] |
| ------ | ------------------------- | --------- | ------------------------- |
| Forest | 200                       | 32        | 34                        |
| Lotus  | 450                       | 128       | 450                       |
| Lotus  | 450                       | 256       | 232                       |

:::note
Lotus snapshots are also not compressed; Forest does it under the hood. Both implementations are able to consume compressed snapshots. This significantly reduces the cost of storing snapshots. While it’s possible to compress the snapshot after it’s been generated, it would increase the operation duration even further.
:::

## Key takeaways

- **Lower tail latency**: comparable P50, but far better P95 and P99, so the slow outliers that most affect reliability are much less frequent.
- **Lower resource usage**: less CPU and memory, with smoother, more predictable load on the machine.
- **Much faster snapshot export**: `7-13x` faster than Lotus while using a fraction of the RAM, with compression handled automatically.

Want to see Forest in action? Check out the [RPC performance Grafana dashboard](https://monitoring.chain.love/public-dashboards/229eaf7ce0224655abe26c288aab914b?orgId=1&refresh=5m&kiosk).
