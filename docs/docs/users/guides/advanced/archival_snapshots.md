---
title: Archival Snapshots
sidebar_position: 2
---

# Archival Snapshots

Forest supports building partial or full archival nodes using **lite** and
**diff** snapshots. This guide explains the snapshot types, how they relate to
each other, and how to use them for common workflows.

## Snapshot types

ChainSafe publishes two kinds of archival snapshots:

| Type     | Naming pattern                                                      | Frequency                      | Contents                                                                                                                          |
| -------- | ------------------------------------------------------------------- | ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------- |
| **Lite** | `forest_snapshot_<network>_<date>_height_<EPOCH>.forest.car.zst`    | Every 30,000 epochs (~10 days) | Complete state trees from `EPOCH - 900` to `EPOCH`, plus the full block header history back to genesis.                           |
| **Diff** | `forest_diff_<network>_<date>_height_<BASE>+<RANGE>.forest.car.zst` | Every 3,000 epochs (~1 day)    | Only the new IPLD key-value pairs added between `BASE` and `BASE + RANGE`. Does **not** contain a complete state tree on its own. |

Archival snapshots are publicly available at:

- Mainnet lite: https://forest-archive.chainsafe.dev/list/mainnet/lite
- Mainnet diff: https://forest-archive.chainsafe.dev/list/mainnet/diff
- Calibnet lite: https://forest-archive.chainsafe.dev/list/calibnet/lite
- Calibnet diff: https://forest-archive.chainsafe.dev/list/calibnet/diff

## How lite and diff snapshots work together

:::warning

A diff snapshot is useless on its own. It **must** be combined with its
matching base lite snapshot (and all intermediate diffs) to form a complete
state tree. Using a diff without the correct base will lead to incomplete state
trees and validation errors such as:

```
failed to lookup actor f410f...
```

or

```
failed to read init actor address map
```

:::

### The golden rule

A **complete state tree** at epoch `E` requires:

1. The **lite snapshot** whose epoch is at or just before `E`, and
2. **All consecutive diff snapshots** that bridge from that lite epoch up to
   `E`.

### Visual example

Consider calibnet snapshots with lite snapshots every 30,000 epochs and diffs
every 3,000 epochs:

```
Lite @3,480,000 ─┬─ Diff @3,480,000+3,000 ─── Diff @3,483,000+3,000 ─── ... ─── Diff @3,507,000+3,000
                 │
                 └─ provides complete state trees from @3,479,100 to @3,480,000
                    each diff extends the complete state trees forward by 3,000 epochs

Lite @3,510,000 ─┬─ Diff @3,510,000+3,000 ─── ...
                 │
                 └─ new base: complete state trees from @3,509,100 to @3,510,000
```

There are 10 diff snapshots between consecutive lite snapshots (30,000 / 3,000
= 10).

## Figuring out which snapshots you need

Given a target epoch `E` and a network, follow these steps:

1. **Find the base lite snapshot.** Take the largest lite epoch that is `≤ E`.
   Lite epochs are multiples of 30,000 for the network.

   ```
   base_epoch = floor(E / 30,000) × 30,000
   ```

2. **List the required diffs.** Starting from `base_epoch`, collect every diff
   snapshot until you reach or pass `E`:

   ```
   diff @base_epoch + 3,000
   diff @(base_epoch + 3,000) + 3,000
   ...
   diff @(last_epoch_before_E) + 3,000
   ```

### Worked example

To get complete state for **calibnet epoch 3,506,992**:

| #   | Snapshot                                                      | Purpose                                  |
| --- | ------------------------------------------------------------- | ---------------------------------------- |
| 1   | `forest_snapshot_calibnet_..._height_3480000.forest.car.zst`  | Base lite (complete state @3,480,000)    |
| 2   | `forest_diff_calibnet_..._height_3480000+3000.forest.car.zst` | State changes through epoch 3,483,000    |
| 3   | `forest_diff_calibnet_..._height_3483000+3000.forest.car.zst` | ... through 3,486,000                    |
| 4   | `forest_diff_calibnet_..._height_3486000+3000.forest.car.zst` | ... through 3,489,000                    |
| 5   | `forest_diff_calibnet_..._height_3489000+3000.forest.car.zst` | ... through 3,492,000                    |
| 6   | `forest_diff_calibnet_..._height_3492000+3000.forest.car.zst` | ... through 3,495,000                    |
| 7   | `forest_diff_calibnet_..._height_3495000+3000.forest.car.zst` | ... through 3,498,000                    |
| 8   | `forest_diff_calibnet_..._height_3498000+3000.forest.car.zst` | ... through 3,501,000                    |
| 9   | `forest_diff_calibnet_..._height_3501000+3000.forest.car.zst` | ... through 3,504,000                    |
| 10  | `forest_diff_calibnet_..._height_3504000+3000.forest.car.zst` | ... through 3,507,000 (covers 3,506,992) |

## Setting up a partial archival node

A partial archival node stores historical data from a chosen starting epoch up
to the present. This is the most common setup for operators who need historical
chain data without syncing from genesis.

### Step 1: Download the snapshots

Download the base lite snapshot and all diff snapshots up to the present. Order
does not matter.

```shell
# Example: calibnet, starting from epoch 3,480,000
# Download the base lite snapshot
aria2c -x5 https://forest-archive.chainsafe.dev/archive/forest/calibnet/lite/forest_snapshot_calibnet_2026-02-22_height_3480000.forest.car.zst

# Download all diff snapshots from 3,480,000 onward
aria2c -x5 https://forest-archive.chainsafe.dev/archive/forest/calibnet/diff/forest_diff_calibnet_2026-02-22_height_3480000+3000.forest.car.zst
aria2c -x5 https://forest-archive.chainsafe.dev/archive/forest/calibnet/diff/forest_diff_calibnet_2026-02-23_height_3483000+3000.forest.car.zst
# ... continue for all diffs up to the present
```

### Step 2: Import snapshots into Forest

Import all snapshot files into the node's CAR database. You can import them in
any order.

```shell
# Initialize the node (creates the database) and stop it
forest --chain calibnet --encrypt-keystore=false --halt-after-import

# Symlink or copy snapshot files into the car_db directory
# (the car_db directory is inside the Forest data directory)
ln -s /path/to/downloaded/snapshots/*.forest.car.zst ~/.local/share/forest/calibnet/car_db/
```

Alternatively, import a recent standard snapshot (for the latest state) and
then add the archival snapshots:

```shell
# Start with a recent standard snapshot
forest --chain calibnet --encrypt-keystore=false --halt-after-import

# Add archival snapshot files to the car_db directory
ln -s /path/to/archival/snapshots/*.forest.car.zst ~/.local/share/forest/calibnet/car_db/
```

### Step 3: Compute states and verify

Start the node and compute states from the lite snapshot's head epoch:

```shell
# Start the node
forest --chain calibnet --encrypt-keystore=false

# Compute states from the base lite epoch forward
# This re-executes all messages and populates the state cache
forest-cli state compute --epoch <LITE_EPOCH> -n <NUMBER_OF_EPOCHS>
```

For example, to compute 200 epochs starting from epoch 3,480,000:

```shell
forest-cli state compute --epoch 3480000 -n 200
```

### Step 4: Validate (optional)

You can validate specific epochs using `forest-dev`:

```shell
forest-dev state validate --chain calibnet --epoch 3506992
```

:::info

Validation can look back up to **2000 epochs**, but each lite snapshot only
contains 900 epochs of state trees. If you are validating an epoch close to the
lite snapshot's head (e.g., within the first few epochs), you may need to also
import the **previous** lite snapshot and its diffs to provide enough state
history. In the worst case, this means downloading the previous segment (1 lite

- 10 diffs).

For epochs well past the lite snapshot's head (more than 2000 epochs ahead),
this is not an issue because the diffs will have extended the state history
sufficiently.

:::

### Backfilling after downtime

If your archival node was offline and missed some epochs, download the missing
diff snapshots (and any new lite snapshots if a new 30,000-epoch boundary was
crossed) and add them to the `car_db` directory. Then restart the node — it
will pick up the new data automatically.

## Retrieving historical data without an archival node

If you only need data at a specific epoch and do not want to run a full
archival node, you can:

1. Download only the base lite snapshot and the diffs up to your target epoch
   (see [Figuring out which snapshots you need](#figuring-out-which-snapshots-you-need)).
2. Import them and compute the state:

```shell
# Initialize and stop
forest --chain calibnet --encrypt-keystore=false --halt-after-import

# Add the snapshots
ln -s /path/to/snapshots/*.forest.car.zst ~/.local/share/forest/calibnet/car_db/

# Start the node
forest --chain calibnet --encrypt-keystore=false

# Compute state at the target epoch range
forest-cli state compute --epoch <LITE_EPOCH> -n <EPOCHS_TO_COMPUTE>

# Validate
forest-dev state validate --chain calibnet --epoch <TARGET_EPOCH>

# Now you can query historical state via RPC
forest-cli state compute --epoch <TARGET_EPOCH>
```

## Common pitfalls

| Problem                                                                | Cause                                                                                                    | Fix                                                                                                   |
| ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `failed to lookup actor f410f...`                                      | Incomplete state tree — missing base lite or intermediate diffs                                          | Ensure the correct base lite snapshot and **all** diffs between it and your target epoch are imported |
| `failed to read init actor address map`                                | Same as above — state tree is partially loaded from a diff without its base                              | Import the matching base lite snapshot                                                                |
| `Parent state root did not match computed state`                       | State was computed from an incomplete state tree                                                         | Re-import with the correct base lite and all diffs, then re-compute                                   |
| `forest-cli state compute` fails but `forest-dev state validate` works | `state compute` requires a running daemon with complete state; `validate` works directly on the database | Import the base lite snapshot covering the epoch and compute from there                               |
| Validation passes with a standard snapshot but fails with diffs        | Diffs were imported without their matching base lite                                                     | Always pair diffs with their base lite snapshot                                                       |

## Merging snapshots into a single file

If you prefer a single snapshot file instead of keeping multiple files in the
CAR database, you can merge them:

```shell
forest-tool archive merge \
  --output-file merged.forest.car.zst \
  forest_snapshot_..._height_30000.forest.car.zst \
  forest_diff_..._height_30000+3000.forest.car.zst \
  forest_diff_..._height_33000+3000.forest.car.zst
```

The output file will contain the combined data and can be used as a standalone
snapshot.

## Generating archival snapshots

New archival snapshots can be generated either manually with `forest-tool
archive export` or automatically with `forest-tool archive sync-bucket`. Both
commands require a large snapshot file as input.

To generate archival snapshots manually, use these settings:

- one lite snapshot every 30,000 epochs,
- one diff snapshot every 3,000 epochs,
- a depth of 900 epochs for the diff snapshots,
- a depth of 900 for the lite snapshots.

Manual generation of archival snapshots should be a last resort. The
`forest-tool archive sync-bucket` command is recommended for generating
archival snapshots.
