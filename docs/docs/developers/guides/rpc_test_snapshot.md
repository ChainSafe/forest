---
title: RPC Test Snapshots
sidebar_position: 2
---

This document describes how to generate and use RPC test snapshots. An RPC test snapshot contains the name of the RPC method, the request payload, the desired response, and the minimal database entries for replaying the request. Thus, it can be used as a regression test or a unit test for an RPC method.

### Generate RPC test dumps

An RPC test dump contains the name of the RPC method, the request payload, and the response of an RPC call. It is used to dump a minimal database snapshot and replay the request in a subsequent process.

To generate test dumps, add `--dump-dir [PATH]` to `forest-tool api compare` command. e.g., `forest-tool api compare forest_snapshot_calibnet_2025-01-15_height_2320334.forest.car.zst --dump-dir /var/tmp/test-dumps`.
This command will create a new directory inside the specified `[PATH]` for `--dump-dir` and will organize the RPC test dumps into two sub-directories based on the validity of the test results:

- valid/: contains test dumps that were valid and passed.
- invalid/: contains test dumps that failed or did not produce valid results.

Note: Running an instance of each Forest and Lotus is required. (Refer to `scripts/tests/api_compare/docker-compose.yml` to setup Lotus with extra environment variables)

### Generate RPC test snapshots

As described above, an RPC test snapshot is generated from an RPC test dump and includes the extra minimal database snapshot in Forest CAR format.

The `forest-tool api generate-test-snapshot` command is for this purpose. Note that it uses the same database as the Forest daemon (db which is also used by the `forest-tool api compare` command in the previous step) to dump the minimal database snapshot. e.g. `forest-tool api generate-test-snapshot --chain calibnet --out-dir /var/tmp/rpc-snapshots /var/tmp/test-dumps/valid/filecoin_stategetallallocations*.json`

### (Optional) compress the test snapshots

A test snapshot generated in the previous step is in JSON format, for easier inspection of the content. The Forest tool set supports `.zst` archives of test snapshots for better disk usage and network bandwidth efficiency. Just run `zstd /var/tmp/rpc-snapshots/*.json`

### Verify the test snapshots

`forest-tool api test /var/tmp/rpc-snapshots/*.json` or `forest-tool api test /var/tmp/rpc-snapshots/*.zst`. As mentioned above, both `.json` and `.json.zst` formats are supported.

### Run the test snapshots in unit tests

- Manual Method
   1. Upload the test snapshots (`.zst` format is recommended) to the Digital Ocean space `forest-snapshots/rpc_test`
   2. Include the file names in `src/tool/subcommands/api_cmd/test_snapshots.txt`
   3. Run the tests:
      ```
      cargo test --lib -- --test rpc_regression_tests --nocapture
      ```

- Using the Script
   1. (One-time setup) Configure your DigitalOcean credentials:
      ```
      s3cmd --configure
      ```
   2. Compress, upload the snapshots, update `test_snapshots.txt` and run the tests:
      ```
      ./scripts/tests/upload_rcpsnaps.sh /var/tmp/rpc-snapshots
      ```

