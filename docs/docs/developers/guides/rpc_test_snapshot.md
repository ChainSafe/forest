---
title: RPC Test Snapshots
sidebar_position: 2
---

This document describes how RPC test snapshots can be generated and used. An RPC test snapshot contains the name of the RPC method, the request payload, the desired response and the minimal database entries for replaying the request. Thus it can be used as a regression test or a unit test for an RPC method.

### Generate RPC test dumps

An RPC test dump contains the name of the RPC method, the request payload and the response of an RPC call. It is used for further dumping a minimal database snapshot for replaying the request in a subsequent process.

To generate test dumps, simply adding `--dump-dir [PATH]` to `forest-tool api compare` command. e.g. `forest-tool api compare forest_snapshot_calibnet_2025-01-15_height_2320334.forest.car.zst --dump-dir /var/tmp/rpc-dump`

### Generate RPC test snapshots

As described above, an RPC test snapshot is generated from an RPC test dump and includes the extra minimal database snapshot in Forest CAR format.

The `forest-tool api generate-test-snapshot` command is for this purpose, note that it takes the database path of the Forest daemon against which the `forest-tool api compare` command in the previous step is run, to dump the minimal database snapshot. e.g. `forest-tool api generate-test-snapshot --db ~/.local/share/forest/calibnet/0.23.3 --chain calibnet --out-dir /var/tmp/rpc-snapshots /var/tmp/test-dumps/filecoin_stategetallallocations*.json`

### (Optional) compress the test snapshots

A test snapshot that is generated in the previous step is in JSON format, for easier inspection of the content. The Forest tool set supports `.zst` archives of test snapshots for better disk usage and network bandwidth efficiency. Simply run `zstd /var/tmp/rpc-snapshots/*.json`

### Verify the test snapshots

`forest-tool api test /var/tmp/rpc-snapshots/*.json` or `forest-tool api test /var/tmp/rpc-snapshots/*.zst`. As mentioned above, both `json` and `.json.zst` formats are supported.

### Run test snapshots in unit tests

- upload the test snapshots (.zst archive is recommended) to the Digital Ocean space `forest-snapshots/rpc_test`
- include the file names in `src/tool/subcommands/api_cmd/test_snapshots.txt`
- run `cargo test --lib -- --test rpc_regression_tests --nocapture`
