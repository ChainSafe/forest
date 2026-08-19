---
title: Enriched Snapshots
---

# Enriched snapshots

A regular snapshot carries the chain spine and recent state trees, but not the
message receipts, events, or a height-to-tipset index. Serving RPC methods that
depend on that data (for example `eth_getTransactionReceipt`, `eth_getLogs`, or
historical `eth_getBlockByNumber`) normally requires a separate
`forest-tool index backfill` pass after import.

An **enriched snapshot** ships that data alongside the base snapshot, so those
methods work immediately after import with no backfill. It is a set of up to
three CAR files that share a common name and differ only by suffix:

| File          | Suffix             | Contents                                                                  |
| ------------- | ------------------ | ------------------------------------------------------------------------- |
| Base          | _(none)_           | Chain spine and recent state trees (a normal snapshot).                   |
| Augmented     | `_receipts_events` | Message receipts and their events for the covered epochs.                 |
| Tipset lookup | `_tipset_lookup`   | A HAMT mapping checkpoint epochs to tipset keys, for fast height lookups. |

For example, a mainnet set at height `6279840`:

```text
forest_snapshot_mainnet_2026-08-14_height_6279840.forest.car.zst
forest_snapshot_mainnet_2026-08-14_height_6279840_receipts_events.forest.car.zst
forest_snapshot_mainnet_2026-08-14_height_6279840_tipset_lookup.forest.car.zst
```

:::info
Enriched snapshots are a recent addition and are not yet listed by the snapshot
service. Discovery is still to be defined; for now the companion files follow the
suffix convention above next to their base snapshot.
:::

## Generating

The augmented and tipset lookup files are produced by the export command as
opt-in extras. Each is written next to the base snapshot with the matching
suffix:

```bash
forest-cli snapshot export --augmented-snapshot --tipset-lookup
```

## Validating

`validate-extended` imports the companion files on top of the base and checks
that every receipt and event loads, and that the lookup HAMT matches the chain:

```bash
forest-tool snapshot validate-extended \
  --base       forest_snapshot_mainnet_2026-08-14_height_6279840.forest.car.zst \
  --augmented  forest_snapshot_mainnet_2026-08-14_height_6279840_receipts_events.forest.car.zst \
  --tipset-lookup forest_snapshot_mainnet_2026-08-14_height_6279840_tipset_lookup.forest.car.zst
```

`--base` is required; `--augmented` and `--tipset-lookup` are each optional, but
at least one must be given.

## Importing into Forest

`--import-snapshot` accepts a single file and the companion CARs have no chain
spine of their own, so only the base goes through it. The companions are loaded
with `forest-tool db`, which writes directly into the node's key-value store and
therefore requires the **daemon to be stopped**.

1. Import the base snapshot normally. This sets the chain head:

   ```bash
   forest --chain mainnet --encrypt-keystore=false \
     --import-snapshot forest_snapshot_mainnet_2026-08-14_height_6279840.forest.car.zst \
     --halt-after-import
   ```

2. Import the augmented snapshot. This streams the receipt and event blocks into
   parity-db (order-independent; `--skip-validation` optional):

   ```bash
   forest-tool db import forest_snapshot_mainnet_2026-08-14_height_6279840_receipts_events.forest.car.zst \
     --chain mainnet
   ```

3. Import the tipset lookup snapshot with its **dedicated** command. The runtime
   reads height-to-tipset from settings-store entries, not from a raw HAMT CAR,
   so a plain `db import` would only add orphan blocks. `import-tipset-lookup`
   validates each entry against tipsets already in the DB (so run it after the
   base), then writes the mappings the node actually reads:

   ```bash
   forest-tool db import-tipset-lookup forest_snapshot_mainnet_2026-08-14_height_6279840_tipset_lookup.forest.car.zst \
     --chain mainnet
   ```

4. Start Forest with `FOREST_ETH_RPC_COMPUTE_BLOOM_ON_MISS=1`. Receipts, events,
   and height lookups are then served straight away, with no
   `forest-tool index backfill` step:

   ```bash
   FOREST_ETH_RPC_COMPUTE_BLOOM_ON_MISS=1 forest --chain mainnet --encrypt-keystore=false
   ```

:::caution
`eth_getBlockByNumber` reads each block's `logsBloom` from a stored bloom index
that is written on tipset execution or by `index backfill`, and that the enriched
snapshot does **not** carry. On a miss the node returns a full (all-ones) bloom,
which fails strict consumers. `FOREST_ETH_RPC_COMPUTE_BLOOM_ON_MISS=1` recomputes
it on the fly from the snapshot's events and state instead.
:::

:::note
The augmented file can alternatively be dropped into the node's `car_db`
directory (`<data-dir>/<chain>/<forest-version>/car_db/`), which Forest loads
read-only at startup. This does **not** work for the tipset lookup file; use
`import-tipset-lookup` for that one.
:::

## Verifying with the RPC checks image

The `ghcr.io/chainsafe/forest-rpc-checks` image runs the same `data.riba.plus`
RPC suite used in CI (see `.github/workflows/external-rpc-checks.yml`). Point it
at a running node and pass the inclusive epoch range to check (it defaults to
`localhost:2345/rpc/v1`):

```bash
docker run --rm --network host ghcr.io/chainsafe/forest-rpc-checks:latest 6279800 6279839
```

Against a node loaded from an enriched snapshot and started with
`FOREST_ETH_RPC_COMPUTE_BLOOM_ON_MISS=1`, the `blocks`, `receipts`, `tipsets`,
and `logs` checks all pass without any prior `index backfill`, since the
receipts, events, and tipset lookups they query are already in the imported CARs.
