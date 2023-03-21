## Forest unreleased

Notable updates:

- Support for nv18.

### Added

- [database] added ParityDb statistics to the stats endpoint.
  [#2444](https://github.com/ChainSafe/forest/pull/2444)
- [api|cli] Add RPC `Filecoin.Shutdown` endpoint and `forest-cli shutdown`
  subcommand. [#2538](https://github.com/ChainSafe/forest/pull/2538)
- [cli] A JavaScript console to interact with Filecoin API.
  [#2492](https://github.com/ChainSafe/forest/pull/2492)
- [docker] Multi-platform Docker image support.
  [#2552](https://github.com/ChainSafe/forest/pull/2552)
- [forest-cli] added `--dry-run` flag to `snapshot export` command.
  [#2549](https://github.com/ChainSafe/forest/pull/2549)
- [forest daemon] Added `--exit-after-init` and `--save-token` flags.
  [#2577](https://github.com/ChainSafe/forest/pull/2577)
- [forest daemon] Support for NV18.
  [#2558](https://github.com/ChainSafe/forest/pull/2558)
  [#2579](https://github.com/ChainSafe/forest/pull/2579)

### Changed

- [database] Move blockstore meta-data to standalone files.
  [2635](https://github.com/ChainSafe/forest/pull/2635)
  [2652](https://github.com/ChainSafe/forest/pull/2652)
- [cli] Remove Forest ctrl-c hard shutdown behavior on subsequent ctrl-c
  signals. [#2538](https://github.com/ChainSafe/forest/pull/2538)
- [libp2p] Use in house bitswap implementation.
  [#2445](https://github.com/ChainSafe/forest/pull/2445)
- [libp2p] Ban peers with duration. Banned peers are automatically unbanned
  after a period of 1h. [#2396](https://github.com/ChainSafe/forest/pull/2396)
- [libp2p] Support multiple listen addr.
  [#2570](https://github.com/ChainSafe/forest/pull/2570)
- [libp2p] Upgrade to v0.51.
  [#2598](https://github.com/ChainSafe/forest/pull/2598)
- [config] `stats` and `compression` keys in `parity_db` section were renamed to
  `enable_statistics` and `compression_type` respectively.
  [#2444](https://github.com/ChainSafe/forest/pull/2444)
- [forest cli] changed how balances are displayed, defaulting to

  - adding metric prefix when it's appropriate to do so, consequently CLI flag
    `--fixed-unit` added to force to show in original `FIL` unit
  - 4 significant digits, consequently CLI flag `--exact-balance` added to force
    full accuracy. [#2385](https://github.com/ChainSafe/forest/pull/2385)

- [config] `download-snapshot` flag was renamed to `auto-download-snapshot`.
  `download_snapshot` key in `client` section in configuration renamed to
  `auto_download_snapshot`.
  [#257](https://github.com/ChainSafe/forest/pull/2457)
- [docker|security] the Forest image is no longer running on a root user but a
  dedicated one. [#2463](https://github.com/ChainSafe/forest/pull/2463)
- [keystore] Allow specifying the encryption passphrase via environmental
  variable. [#2514](https://github.com/ChainSafe/forest/pull/2514)
- [forest daemon] The `--skip-load` flag must be now called with a boolean
  indicating its value. [#2577](https://github.com/ChainSafe/forest/pull/2577)
- [cli] Calibnet network needs to be specified for most commands, including
  `sync wait` and `snapshot export`.
  [#2579](https://github.com/ChainSafe/forest/pull/2579)
- [daemon] Switch to ParityDb as the default backend for the Forest daemon. All
  clients must re-import the snapshot. The old database must be deleted
  manually - it is located in
  `$(forest-cli config dump | grep data_dir | cut -d' ' -f3)/<NETWORK>/rocksdb`.
  [#2606](https://github.com/ChainSafe/forest/pull/2606)
- [database] Move the genesis header and heaviest tipset keys from the database
  to files. [#2635](https://github.com/ChainSafe/forest/pull/2635)

### Removed

- [forest daemon] Removed `--halt-after-import` and `--auto-download-snapshot`
  from configuration. They are now strictly a CLI option.
  [#2577](https://github.com/ChainSafe/forest/pull/2577)

### Fixed

- [libp2p] Properly cancel bitswap queries that are not responded to after a
  period. [#2399](https://github.com/ChainSafe/forest/pull/2399)
- [console ui] `Scanning Blockchain` progess bar never hits 100% during snapshot
  import. [#2403](https://github.com/ChainSafe/forest/pull/2403)
- [forest daemon] forest daeamon crashes on sending bitswap requests.
  [#2419](https://github.com/ChainSafe/forest/pull/2419)
- [version] The version shown in `--help` was stuck at `0.4.1`. Now all binaries
  and crates in the project will follow a standard version, based on the release
  tag. [#2487](https://github.com/ChainSafe/forest/pull/2487)
- [forest] Failing snapshot fetch resulting in daemon crash in one attempt.
  [#2571](https://github.com/ChainSafe/forest/pull/2571)
- [forest-cli] corrected counts displayed when using
  `forest-cli --chain <chain> sync wait`.
  [#2654](https://github.com/ChainSafe/forest/pull/2654)
- [forest-cli] Fix snapshot export when running on a system with a separate
  temporary filesystem. [#2693](https://github.com/ChainSafe/forest/pull/2693)

## Forest v0.6.0 (2023-01-06)

Notable updates:

- Added support for the new Protocol Labs snapshot service.
- Several improvements to logging (including integration with Grafana Loki) and
  error handling.

### Added

- New daemon option flag `--log-dir` for log file support.
- New ParityDb section in configuration (including statistics and compression
  settings).
- Integration with Grafana Loki for more advanced log filtering and
  summarization.
- Peer tipset epoch now in metrics.

### Changed

- Several improvements to error handling.
- Docker images are now tagged with version (eg `0.6.0`) and date (eg
  `2023-01-05-commit_hash`).
- Upgraded toolchain channel to `nightly-2022-12-15`.
- Migrated logging infrastructure from `log` to `tracing`.
- Fixed bug triggered by large network messages.
- Fixed bug where cursor would be hidden after keyboard interrupt.
- Allow `-—height` flag to take relative offsets for validating the tipsets in a
  snapshot.
- Fixed issue with invalid snapshot exports; messages were accidentally removed
  from snapshots, making them invalid.
- Updated `snapshot fetch` subcommands to support the new Protocol Labs snapshot
  service.
- Fixed RPC `net disconnect` endpoint (a bug was returning a JSON RPC error when
  running `forest-cli net disconnect` and preventing proper peer disconnection).
- Corrected RPC serialization of FIL balances (a bug was preventing display of
  floating point balance using `forest-cli wallet list`).

### Removed

- RocksDB check for low file descriptor limit.
- Unused RPC endpoints.

## Forest v0.5.1 (2022-12-01)

### Changed

- Restore progress indicators that were accidentally broken.

## Forest v0.5.0 (2022-12-01)

Notable updates:

- Support for nv17.
- Forest was split into two programs: a Filecoin node (forest), and a control
  program (forest-cli).
- Improved snapshot importing performance: ~75% reduction in snapshot import
  time.
- Improved code building time: ~45% reduction in build time.
- Code coverage increased from 32% to 63%.

### Added

- Support for nv17 on both calibnet and mainnet.
- Experimental support for ParityDB.
- Improved snapshot handling via the `forest-cli snapshot` commands.
- Support using `aria2` for faster snapshot downloads.
- Support for sending FIL.

### Changed

- Replace async_std with tokio.
- Significantly improve tracked performance metrics.
- Gracefully shutdown the database on sigterm and sighup.
- Fix gas charging issue that caused state-root mismatches on mainnet.
- Snapshots are automatically downloaded if the database is empty.
- Improve error messages if a snapshot doesn't match the requested network.
- Add `--color=[always;auto;never]` flag.

### Removed

- Fat snapshots (snapshots that contain all transaction receipts since genesis)
  have been deprecated in favor of slim snapshots where receipts are downloaded
  on demand.
- All security advistory exceptions. Forest's dependencies are now free of known
  vulnerabilities.

## Forest v0.4.1 (2022-10-04)

### Changed

- Fix bug in handling of blockchain forks.

## Forest v0.4.0 (2022-09-30)

Notable updates:

- Support for nv16.
- Built-in method of downloading snapshots.
- Vastly improved automated testing.

### Added

- New `forest chain export` command for generating snapshots.
- New `forest chain fetch` command for downloading recent snapshots.
- Logging settings are now part of the configuration file rather than only being
  accessible through an environment variable.
- A `--detach` flag for running the Forest node in the background.
- A `--halt-after-import` for exiting Forest directly after importing a
  snapshot.
- Delegated Consensus: A consensus mode useful for testing.
- FIP-0023: Break ties between tipsets of equal weight.

### Changed

- Improve error messages if Forest isn't initiated with a valid database.
- Formatting clean-up in the forest wallet.
- Improved pretty-printing of debugging statediffs.
- Several dozen spelling fixes in the documentation.
- Fixed dead links in documentation (with automated detection).
- Avoided a segmentation fault caused by an improper shutdown of the database.
- Bump required rust version from nightly-2022-09-08 to nightly-2022-09-28.

### Removed

- Support for the `sled` database.

## Forest v0.3.0 (2022-07-04)

Notable updates:

- Support nv15 entirely through the FVM.
- Resolve two security concerns by removing legacy code (RUSTSEC-2020-0071 and
  RUSTSEC-2021-0130).
- Fixed Docker image and released it to GH container registry.
- Network selection (ie mainnet vs testnet) moved to a CLI flag rather than a
  compile-time flag.

## Forest v0.2.2 _alpha_ (2022-04-06)

Forest v0.2.2 alpha is a service release improving performance and stability.
This release supports Filecoin network version 14.

Notable updates:

- Forest now supports Calibnet: `make calibnet` (nv14)
- FVM is available both native and as external crate:
  [ref-fvm](https://github.com/filecoin-project/ref-fvm)
- Reading config from a default config location unless a file is specified.
- Improved logging and display of synchronization progress.
- Defaulting to Rust Edition 2021 from now on.

All changes:

- Log: don't override default filters (#1504) by @jdjaustin in
  [#1530](https://github.com/ChainSafe/forest/pull/1530)
- Crates: bump wasmtime by @q9f in
  [#1526](https://github.com/ChainSafe/forest/pull/1526)
- Ci: add wasm target to release script by @q9f in
  [#1524](https://github.com/ChainSafe/forest/pull/1524)
- Ci: add codecov target threshold tolerance of 1% by @q9f in
  [#1525](https://github.com/ChainSafe/forest/pull/1525)
- Node: demote noisy warnings to debug by @q9f in
  [#1518](https://github.com/ChainSafe/forest/pull/1518)
- Workaround fix for prometheus endpoint by @LesnyRumcajs in
  [#1516](https://github.com/ChainSafe/forest/pull/1516)
- Fixed bug label for bug template by @LesnyRumcajs in
  [#1514](https://github.com/ChainSafe/forest/pull/1514)
- Crates: purge unused dependencies by @q9f in
  [#1509](https://github.com/ChainSafe/forest/pull/1509)
- Github: update code owners by @q9f in
  [#1507](https://github.com/ChainSafe/forest/pull/1507)
- Ci: enable rustc version trinity for builds by @q9f in
  [#1506](https://github.com/ChainSafe/forest/pull/1506)
- Crates: bump dependencies by @q9f in
  [#1503](https://github.com/ChainSafe/forest/pull/1503)
- Re-use some code from ref-fvm by @LesnyRumcajs in
  [#1500](https://github.com/ChainSafe/forest/pull/1500)
- Connor/default config location by @connormullett in
  [#1494](https://github.com/ChainSafe/forest/pull/1494)
- Deps: simplify os dependencies by @q9f in
  [#1496](https://github.com/ChainSafe/forest/pull/1496)
- Use exports from ref-fvm by @LesnyRumcajs in
  [#1495](https://github.com/ChainSafe/forest/pull/1495)
- Start the prometheus server before loading snapshots. by @lemmih in
  [#1484](https://github.com/ChainSafe/forest/pull/1484)
- Config dump with tests by @LesnyRumcajs in
  [#1485](https://github.com/ChainSafe/forest/pull/1485)
- Use the v6 version of the actor's bundle. by @lemmih in
  [#1474](https://github.com/ChainSafe/forest/pull/1474)
- Exposed more rocksdb options, increased max files by @LesnyRumcajs in
  [#1481](https://github.com/ChainSafe/forest/pull/1481)
- Parametrize current rocksdb settings by @LesnyRumcajs in
  [#1479](https://github.com/ChainSafe/forest/pull/1479)
- Use progress bars when downloading headers and scanning the blockchain. by
  @lemmih in [#1480](https://github.com/ChainSafe/forest/pull/1480)
- Night job scripts by @LesnyRumcajs in
  [#1475](https://github.com/ChainSafe/forest/pull/1475)
- Add more metrics of syncing by @LesnyRumcajs in
  [#1467](https://github.com/ChainSafe/forest/pull/1467)
- Limit RocksDB to 200 open files. by @lemmih in
  [#1468](https://github.com/ChainSafe/forest/pull/1468)
- Ci: Include conformance tests in code coverage results by @lemmih in
  [#1470](https://github.com/ChainSafe/forest/pull/1470)
- Show a progressbar when downloading tipset headers. by @lemmih in
  [#1469](https://github.com/ChainSafe/forest/pull/1469)
- Add 'fvm' backend in parallel to our native backend. by @lemmih in
  [#1403](https://github.com/ChainSafe/forest/pull/1403)
- Update regex to v1.5.5 (from 1.5.4) to avoid performance vulnerability. by
  @lemmih in [#1472](https://github.com/ChainSafe/forest/pull/1472)
- Ci: Allow codecov policies to fail. by @lemmih in
  [#1471](https://github.com/ChainSafe/forest/pull/1471)
- Improve docker-compose for monitoring stack by @LesnyRumcajs in
  [#1461](https://github.com/ChainSafe/forest/pull/1461)
- Revert "Enforce max length when serializing/deserializing arrays" by @lemmih
  in [#1462](https://github.com/ChainSafe/forest/pull/1462)
- Introduce serde_generic_array by @clearloop in
  [#1434](https://github.com/ChainSafe/forest/pull/1434)
- Fixed new clippy warnings by @LesnyRumcajs in
  [#1449](https://github.com/ChainSafe/forest/pull/1449)
- Improve license check script by @LesnyRumcajs in
  [#1443](https://github.com/ChainSafe/forest/pull/1443)
- Elmattic/actors review f26 by @elmattic in
  [#1340](https://github.com/ChainSafe/forest/pull/1340)
- Calibnet Support by @connormullett in
  [#1370](https://github.com/ChainSafe/forest/pull/1370)
- Blockchain/sync: demote chain exchange warning to debug message by @q9f in
  [#1439](https://github.com/ChainSafe/forest/pull/1439)
- Fix clippy fiascoes introduced in #1437 by @LesnyRumcajs in
  [#1438](https://github.com/ChainSafe/forest/pull/1438)
- Fix signature verification fiasco by @LesnyRumcajs in
  [#1437](https://github.com/ChainSafe/forest/pull/1437)
- Clippy for tests by @LesnyRumcajs in
  [#1436](https://github.com/ChainSafe/forest/pull/1436)
- Rustc: switch to rust edition 2021 by @q9f in
  [#1429](https://github.com/ChainSafe/forest/pull/1429)
- Forest: bump version to 0.2.2 by @q9f in
  [#1428](https://github.com/ChainSafe/forest/pull/1428)
- Move from chrono to time crate by @LesnyRumcajs in
  [#1426](https://github.com/ChainSafe/forest/pull/1426)

## Forest v0.2.1 _alpha_ (2022-02-14)

Forest v0.2.1 alpha is a service release improving performance and stability.

All changes:

- Ci: fix documentation in release workflow by @q9f in
  [#1427](https://github.com/ChainSafe/forest/pull/1427)
- Feat(encoding): add max length check for bytes by @clearloop in
  [#1399](https://github.com/ChainSafe/forest/pull/1399)
- Add assert in debug mode and tests by @elmattic in
  [#1416](https://github.com/ChainSafe/forest/pull/1416)
- Add shellcheck to CI by @LesnyRumcajs in
  [#1423](https://github.com/ChainSafe/forest/pull/1423)
- Fail CI on failed fmt or other linting file changes by @LesnyRumcajs in
  [#1422](https://github.com/ChainSafe/forest/pull/1422)
- Crates: replace monkey-patched cs\_\* crates by upstream deps by @q9f in
  [#1414](https://github.com/ChainSafe/forest/pull/1414)
- Add LesnyRumcajs to CODEOWNERS by @LesnyRumcajs in
  [#1425](https://github.com/ChainSafe/forest/pull/1425)
- Ci: temporarily ignore RUSTSEC-2022-0009 by @q9f in
  [#1424](https://github.com/ChainSafe/forest/pull/1424)
- Vm/actor: remove unused fields in paych actor tests by @q9f in
  [#1415](https://github.com/ChainSafe/forest/pull/1415)
- Forest: bump version to 0.2.1 by @q9f in
  [#1417](https://github.com/ChainSafe/forest/pull/1417)
- Fix exit code mismatch by @noot in
  [#1412](https://github.com/ChainSafe/forest/pull/1412)
- Improve snapshot parsing performance by ~2.5x by @lemmih in
  [#1408](https://github.com/ChainSafe/forest/pull/1408)
- Update conformance test vectors (and fix test driver) by @lemmih in
  [#1404](https://github.com/ChainSafe/forest/pull/1404)
- Use human readable units when loading snapshots. by @lemmih in
  [#1407](https://github.com/ChainSafe/forest/pull/1407)
- Chore: bump rocksdb to 0.17 by @q9f in
  [#1398](https://github.com/ChainSafe/forest/pull/1398)
- Include network in forest version string. by @lemmih in
  [#1401](https://github.com/ChainSafe/forest/pull/1401)
- Fix 1369 by @willeslau in
  [#1397](https://github.com/ChainSafe/forest/pull/1397)
- Move `/docs` to `/documentation` by @connormullett in
  [#1390](https://github.com/ChainSafe/forest/pull/1390)

## Forest v0.2.0 _alpha_ (2022-01-25)

ChainSafe System's second _alpha_ release of the _Forest_ Filecoin Rust protocol
implementation. This release fixes a series of bugs and performance issues and
introduces, among others, support for:

- Full mainnet compatibility
- Filecoin network version 14 "Chocolate"
- Forest actors version 6
- Further audit fixes

To compile release binaries, checkout the `v0.2.0` tag and build with the
`release` feature.

```shell
git checkout v0.2.0
cargo build --release --bin forest --features release
./target/release/forest --help
```

All changes:

- Release forest v0.2.0 alpha
  ([#1393](https://github.com/ChainSafe/forest/pull/1393)
- C1 actors review ([#1368](https://github.com/ChainSafe/forest/pull/1368))
- Fix encoding size constraints for BigInt and BigUint not enforced
  ([#1367](https://github.com/ChainSafe/forest/pull/1367))
- Fix typo when running conformance tests.
  ([#1394](https://github.com/ChainSafe/forest/pull/1394))
- Auto-detect available cores on Linux and MacOS.
  ([#1387](https://github.com/ChainSafe/forest/pull/1387)
- Remove unused lint exceptions.
  ([#1385](https://github.com/ChainSafe/forest/pull/1385)
- B4 fix: fixing by adding max index computation in bitfield validation
  ([#1344](https://github.com/ChainSafe/forest/pull/1344))
- Ci: run github actions on buildjet
  ([#1366](https://github.com/ChainSafe/forest/pull/1366))
- Ci: documentation dry-run for PRs.
  ([#1383](https://github.com/ChainSafe/forest/pull/1383))
- Use pre-made action to deploy documentation to gh-pages.
  ([#1380](https://github.com/ChainSafe/forest/pull/1380))
- Networks: Show an informative error message if the selected feature set is
  invalid. ([#1373](https://github.com/ChainSafe/forest/pull/1373))
- Disable test 'test_optimal_message_selection3' because it is inconsistent.
  ([#1381](https://github.com/ChainSafe/forest/pull/1381))
- Add David to repo maintainers
  ([#1374](https://github.com/ChainSafe/forest/pull/1374))
- Apply lints from rust-1.58
  ([#1378](https://github.com/ChainSafe/forest/pull/1378))
- Catch panic in verify_window_post
  ([#1365](https://github.com/ChainSafe/forest/pull/1365))
- Make 'base64' dependency for key_management no longer optional
  ([#1372](https://github.com/ChainSafe/forest/pull/1372))
- Fix snapshot get in docs
  ([#1353](https://github.com/ChainSafe/forest/pull/1353))
- Fix market logic ([#1356](https://github.com/ChainSafe/forest/pull/1356))
- V6: fix market and power actors to match go
  ([#1348](https://github.com/ChainSafe/forest/pull/1348))
- F28 fix ([#1343](https://github.com/ChainSafe/forest/pull/1343))
- Fix: F25 ([#1342](https://github.com/ChainSafe/forest/pull/1342))
- Ci: --ignore RUSTSEC-2021-0130
  ([#1350](https://github.com/ChainSafe/forest/pull/1350))
- Drand v14 update: fix fetching around null tipsets
  ([#1339](https://github.com/ChainSafe/forest/pull/1339))
- Fix v6 market actor bug
  ([#1341](https://github.com/ChainSafe/forest/pull/1341))
- F27 fix ([#1328](https://github.com/ChainSafe/forest/pull/1328))
- F17 fix ([#1324](https://github.com/ChainSafe/forest/pull/1324))
- Laudiacay/actors review f23
  ([#1325](https://github.com/ChainSafe/forest/pull/1325))
- Fix market actor publish_storage_deals
  ([#1327](https://github.com/ChainSafe/forest/pull/1327))
- Remove .swp ([#1326](https://github.com/ChainSafe/forest/pull/1326))
- F24 fix ([#1323](https://github.com/ChainSafe/forest/pull/1323))
- F9 fix ([#1315](https://github.com/ChainSafe/forest/pull/1315))
- F20: Fix expiration set validation order
  ([#1322](https://github.com/ChainSafe/forest/pull/1322))
- F13 fix ([#1313](https://github.com/ChainSafe/forest/pull/1313))
- F21 fix ([#1311](https://github.com/ChainSafe/forest/pull/1311))
- F11 fix ([#1312](https://github.com/ChainSafe/forest/pull/1312))
- F15 fix ([#1314](https://github.com/ChainSafe/forest/pull/1314))
- F18, F19 fix ([#1321](https://github.com/ChainSafe/forest/pull/1321))
- Nv14: implement v6 actors
  ([#1260](https://github.com/ChainSafe/forest/pull/1260))
- Add to troubleshooting docs
  ([#1282](https://github.com/ChainSafe/forest/pull/1282))
- F12 fix ([#1290](https://github.com/ChainSafe/forest/pull/1290))
- F1 fix ([#1293](https://github.com/ChainSafe/forest/pull/1293))
- F16: Fix improper use of assert macro
  ([#1310](https://github.com/ChainSafe/forest/pull/1310))
- F14: Fix missing continue statement
  ([#1309](https://github.com/ChainSafe/forest/pull/1309))
- F10 fix ([#1308](https://github.com/ChainSafe/forest/pull/1308))
- F7: Fix incorrect error codes
  ([#1297](https://github.com/ChainSafe/forest/pull/1297))
- F8: Add missing decrement for miner_count
  ([#1298](https://github.com/ChainSafe/forest/pull/1298))
- F6: Fix incorrect error code
  ([#1296](https://github.com/ChainSafe/forest/pull/1296))
- F5: Fix proposal check in market actor
  ([#1295](https://github.com/ChainSafe/forest/pull/1295))
- Remove redundant validation code and update error message to be same as in
  spec actors ([#1294](https://github.com/ChainSafe/forest/pull/1294))
- F3: fix logic to be the same as in the spec actors
  ([#1292](https://github.com/ChainSafe/forest/pull/1292))
- Attempt to improve gh actions time
  ([#1319](https://github.com/ChainSafe/forest/pull/1319))
- Fix clippy errors for the new cargo 1.57.0
  ([#1316](https://github.com/ChainSafe/forest/pull/1316))
- Ci: add gh actions workflows
  ([#1317](https://github.com/ChainSafe/forest/pull/1317))
- Fix: audit issue F2 ([#1289](https://github.com/ChainSafe/forest/pull/1289))
- Update codeowners ([#1306](https://github.com/ChainSafe/forest/pull/1306))
- Add Guillaume to code owners
  ([#1283](https://github.com/ChainSafe/forest/pull/1283))
- .circleci: Remove extra step for docs
  ([#1251](https://github.com/ChainSafe/forest/pull/1251))
- .circleci: Build and push mdbook
  ([#1250](https://github.com/ChainSafe/forest/pull/1250))
- Add MdBook Documentation
  ([#1249](https://github.com/ChainSafe/forest/pull/1249))
- Docs: add release notes
  ([#1246](https://github.com/ChainSafe/forest/pull/1246))

## Forest v0.1.0 _alpha_ (2021-10-19)

ChainSafe System's first _alpha_ release of the _Forest_ Filecoin Rust protocol
implementation.

- It synchronizes and verifies the latest Filecoin main network and is able to
  query the latest state.
- It implements all core systems of the Filecoin protocol specification exposed
  through a command-line interface.
- The set of functionalities for this first alpha-release include: Message Pool,
  State Manager, Chain and Wallet CLI functionality, Prometheus Metrics, and a
  JSON-RPC Server.

To compile release binaries, checkout the `v0.1.0` tag and build with the
`release` feature.

```shell
git checkout v0.1.0
cargo build --release --bin forest --features release
./target/release/forest --help
```

The Forest mono-repository contains ten main components (in logical order):

- `forest`: the command-line interface and daemon (1 crate/workspace)
- `node`: the networking stack and storage (7 crates)
- `blockchain`: the chain structure and synchronization (6 crates)
- `vm`: state transition and actors, messages, addresses (9 crates)
- `key_management`: Filecoin account management (1 crate)
- `crypto`: cryptographic functions, signatures, and verification (1 crate)
- `encoding`: serialization library for encoding and decoding (1 crate)
- `ipld`: the IPLD model for content-addressable data (9 crates)
- `types`: the forest types (2 crates)
- `utils`: the forest toolbox (12 crates)

All initial change sets:

- `cd33929e` Ci: ignore cargo audit for RUSTSEC-2020-0159
  ([#1245](https://github.com/ChainSafe/forest/pull/1245)) (Afr Schoe)
- `d7e816a7` Update Libp2p to 0.40.0-RC.1
  ([#1243](https://github.com/ChainSafe/forest/pull/1243)) (Eric Tu)
- `a33328c9` Mpool CLI Commands
  ([#1203](https://github.com/ChainSafe/forest/pull/1203)) (Connor Mullett)
- `9d4b5291` Create new_issue.md
  ([#1193](https://github.com/ChainSafe/forest/pull/1193)) (Lee Raj)
- `60910979` Actor_name_by_code
  ([#1218](https://github.com/ChainSafe/forest/pull/1218)) (Eric Tu)
- `5845cdf7` Bump libsecp256k1 and statrs
  ([#1244](https://github.com/ChainSafe/forest/pull/1244)) (Eric Tu)
- `a56e4a53` Fix stable clippy::needless_collect
  ([#1238](https://github.com/ChainSafe/forest/pull/1238)) (Afr Schoe)
- `4eb74f90` Fix stable clippy::needless_borrow
  ([#1236](https://github.com/ChainSafe/forest/pull/1236)) (Afr Schoe)
- `5006e62a` Clippy: avoid contiguous acronyms
  ([#upper_case_acronyms](https://github.com/ChainSafe/forest/pull/upper_case_acronyms))
  ([#1239](https://github.com/ChainSafe/forest/pull/1239)) (Afr Schoe)
- `8543b3fb` Connor/state cli
  ([#1219](https://github.com/ChainSafe/forest/pull/1219)) (Connor Mullett)
- `b40f8d11` Fix Deadlock when using Rayon
  ([#1240](https://github.com/ChainSafe/forest/pull/1240)) (Eric Tu)
- `0e816c8a` Clippy: remove redundant enum variant names (`enum_variant_names`)
  ([#1237](https://github.com/ChainSafe/forest/pull/1237)) (Afr Schoe)
- `db5bb065` Cli: use cargo package version environment data in cli options
  struct ([#1229](https://github.com/ChainSafe/forest/pull/1229)) (Afr Schoe)
- `28f7d83f` Rust: default to stable toolchain instead of pinned version
  ([#1228](https://github.com/ChainSafe/forest/pull/1228)) (Afr Schoe)
- `70f26c29` Circleci: prepare build matrix
  ([#1233](https://github.com/ChainSafe/forest/pull/1233)) (Afr Schoe)
- `d9a4df14` Scripts: fix copyright header years
  ([#1230](https://github.com/ChainSafe/forest/pull/1230)) (Afr Schoe)
- `ccf1ac11` Return Ok when validating drand beacon entries similar to how Lotus
  does as per the audit recommendation.
  ([#1206](https://github.com/ChainSafe/forest/pull/1206)) (Hunter Trujillo)
- `f5fe14d2` [Audit fixes] FOR-03 - Inconsistent Deserialization of Randomness
  ([#1205](https://github.com/ChainSafe/forest/pull/1205)) (Hunter Trujillo)
- `32a9ae5f` Rest of V5 Updates
  ([#1217](https://github.com/ChainSafe/forest/pull/1217)) (Eric Tu)
- `e6e1c8ad` API_IMPLEMENTATION.md build script formatting improvements
  ([#1210](https://github.com/ChainSafe/forest/pull/1210)) (Hunter Trujillo)
- `1e88b095` For 01 ([#1188](https://github.com/ChainSafe/forest/pull/1188))
  (Jorge Olivero)
- `881d8f23` FIP 0013 Aggregate Seal Verification
  ([#1185](https://github.com/ChainSafe/forest/pull/1185)) (Eric Tu)
- `ea98ea2a` FIP 0008 Batch Pre Commits
  ([#1189](https://github.com/ChainSafe/forest/pull/1189)) (Eric Tu)
- `a134d5ed` Multi-key import feature
  ([#1201](https://github.com/ChainSafe/forest/pull/1201)) (Elvis)
- `0c447d4c` OpenRPC schema parsing & generation
  ([#1194](https://github.com/ChainSafe/forest/pull/1194)) (Hunter Trujillo)
- `bf3936a2` Connor/cli smoke test
  ([#1196](https://github.com/ChainSafe/forest/pull/1196)) (Connor Mullett)
- `7d5b3333` Reference files with PathBuf instead of Strings
  ([#1200](https://github.com/ChainSafe/forest/pull/1200)) (Elvis)
- `3771568c` Remove loopback and duplicate addrs in `net peers` output
  ([#1199](https://github.com/ChainSafe/forest/pull/1199)) (Francis Murillo)
- `ffc30193` Constant consensus fault reward
  ([#1190](https://github.com/ChainSafe/forest/pull/1190)) (Eric Tu)
- `d88ea8d1` Added the check for config file via Env Var
  ([#1197](https://github.com/ChainSafe/forest/pull/1197)) (Elvis)
- `d4a1d044` Chain Sync CLI Commands
  ([#1175](https://github.com/ChainSafe/forest/pull/1175)) (Connor Mullett)
- `698cf3c3` Additional Net RPC API & CLI Methods
  ([#1167](https://github.com/ChainSafe/forest/pull/1167)) (Hunter Trujillo)
- `32656db9` `auth api-info`
  ([#1172](https://github.com/ChainSafe/forest/pull/1172)) (Connor Mullett)
- `90ab8650` FOR-06 fix: indexmap version bump and MSRV update
  ([#1180](https://github.com/ChainSafe/forest/pull/1180)) (creativcoder)
- `d1d6f640` Update Runtime to Support V5 Actors
  ([#1173](https://github.com/ChainSafe/forest/pull/1173)) (Eric Tu)
- `085ee872` Bugfix:vm:run cron for null rounds
  ([#1177](https://github.com/ChainSafe/forest/pull/1177)) (detailyang)
- `07499a3f` Chore:build:tweak interopnet compile flags
  ([#1178](https://github.com/ChainSafe/forest/pull/1178)) (detailyang)
- `933503e2` Hotfix:metrics:change prometheus response type
  ([#1169](https://github.com/ChainSafe/forest/pull/1169)) (detailyang)
- `99fa3864` Metrics ([#1102](https://github.com/ChainSafe/forest/pull/1102))
  (Jorge Olivero)
- `02791b92` Implement network version 12 state migration / actors v4 migration
  ([#1101](https://github.com/ChainSafe/forest/pull/1101)) (creativcoder)
- `d144eac8` Fix wallet verify
  ([#1170](https://github.com/ChainSafe/forest/pull/1170)) (Connor Mullett)
- `f07f0278` Hotfix:fix passphrase fake confirm
  ([#1168](https://github.com/ChainSafe/forest/pull/1168)) (detailyang)
- `132884d8` Chore:interopnet&devnet:fix build error
  ([#1162](https://github.com/ChainSafe/forest/pull/1162)) (detailyang)
- `992e69e3` FOR-15 fix approx_cmp in msg_chain.rs
  ([#1160](https://github.com/ChainSafe/forest/pull/1160)) (creativcoder)
- `34799734` Wallet CLI Implementation
  ([#1128](https://github.com/ChainSafe/forest/pull/1128)) (Connor Mullett)
- `f698ba88` [Audit fixes] FOR-02: Inconsistent Deserialization of Address ID
  ([#1149](https://github.com/ChainSafe/forest/pull/1149)) (Hunter Trujillo)
- `e50d2ae8` [Audit fixes] FOR-16: Unnecessary Extensive Permissions for Private
  Keys ([#1151](https://github.com/ChainSafe/forest/pull/1151)) (Hunter
  Trujillo)
- `665ca476` Subtract 1 ([#1152](https://github.com/ChainSafe/forest/pull/1152))
  (Eric Tu)
- `4047ff5e` 3 -> 4 ([#1153](https://github.com/ChainSafe/forest/pull/1153))
  (Eric Tu)
- `446bea40` Swap to asyncronous_codec and bump futures_cbor_codec
  ([#1163](https://github.com/ChainSafe/forest/pull/1163)) (Eric Tu)
- `e4e6711b` Encrypted keystore now defaults to enabled. Warn the user if using
  an unencrypted keystore.
  ([#1150](https://github.com/ChainSafe/forest/pull/1150)) (Hunter Trujillo)
- `9b2a03a6` Add rust-toolchain file
  ([#1132](https://github.com/ChainSafe/forest/pull/1132)) (Hunter Trujillo)
- `6f9edae8` Fix P2P random walk logic
  ([#1125](https://github.com/ChainSafe/forest/pull/1125)) (Fraccaroli
  Gianmarco)
- `87f61d20` Spelling and typos
  ([#1126](https://github.com/ChainSafe/forest/pull/1126)) (Kirk Baird)
- `69d52cbd` RPC API w/ Permissions handling
  ([#1122](https://github.com/ChainSafe/forest/pull/1122)) (Hunter Trujillo)
- `81080179` Import/Export StateTree for Testing
  ([#1114](https://github.com/ChainSafe/forest/pull/1114)) (Eric Tu)
- `b75a4f31` Improve CLI printing and RPC error handling.
  ([#1121](https://github.com/ChainSafe/forest/pull/1121)) (Hunter Trujillo)
- `a8931e2a` Enable Gossip Scoring
  ([#1115](https://github.com/ChainSafe/forest/pull/1115)) (Eric Tu)
- `c337d3bf` V5 Actors Prep
  ([#1116](https://github.com/ChainSafe/forest/pull/1116)) (Eric Tu)
- `0a20e468` JSON-RPC Client with FULLNODE_API_INFO config & JWT support
  ([#1100](https://github.com/ChainSafe/forest/pull/1100)) (Hunter Trujillo)
- `f87d0baf` Resolve Stack Overflow
  ([#1103](https://github.com/ChainSafe/forest/pull/1103)) (Eric Tu)
- `bd90aa65` Tidy up submitwindowedpost
  ([#1099](https://github.com/ChainSafe/forest/pull/1099)) (Kirk Baird)
- `36a18656` Add Prometheus server
  ([#1098](https://github.com/ChainSafe/forest/pull/1098)) (Jorge Olivero)
- `ff6776ca` Base64 encode persistent keystore
  ([#1092](https://github.com/ChainSafe/forest/pull/1092)) (Connor Mullett)
- `c02669fa` Remove tide-websockets-sink fork
  ([#1089](https://github.com/ChainSafe/forest/pull/1089)) (Hunter Trujillo)
- `84ab31b0` Parallelize tipset processing
  ([#1081](https://github.com/ChainSafe/forest/pull/1081)) (Jorge Olivero)
- `d245f14c` Encrypted Key Store
  ([#1078](https://github.com/ChainSafe/forest/pull/1078)) (Connor Mullett)
- `34740715` Actors v4 (Network v12)
  ([#1087](https://github.com/ChainSafe/forest/pull/1087)) (Eric Tu)
- `946f4510` Implement optimal message selection
  ([#1086](https://github.com/ChainSafe/forest/pull/1086)) (creativcoder)
- `e8c1b599` Ignore If txn_id Existed Or Not When Deleting
  ([#1082](https://github.com/ChainSafe/forest/pull/1082)) (Eric Tu)
- `0a134afa` Devnet Build
  ([#1073](https://github.com/ChainSafe/forest/pull/1073)) (Eric Tu)
- `77f8495e` Remove check for empty_params
  ([#1079](https://github.com/ChainSafe/forest/pull/1079)) (Eric Tu)
- `bb7034ac` Minor fixes and touch-ups
  ([#1074](https://github.com/ChainSafe/forest/pull/1074)) (François Garillot)
- `674c3b39` HTTP JWT validation
  ([#1072](https://github.com/ChainSafe/forest/pull/1072)) (Hunter Trujillo)
- `4c9856dc` Disable MacOS CI for now. Looked into it a bit, not sure what other
  better solutions we have.
  ([#1077](https://github.com/ChainSafe/forest/pull/1077)) (Hunter Trujillo)
- `8a3823c3` Add @connormullett to Code Owners
  ([#1076](https://github.com/ChainSafe/forest/pull/1076)) (Hunter Trujillo)
- `e52b34d0` Remove a couple unnecessary panics
  ([#1075](https://github.com/ChainSafe/forest/pull/1075)) (François Garillot)
- `3606c3f9` Fix Error Handling in Load Deadlines
  ([#1071](https://github.com/ChainSafe/forest/pull/1071)) (Eric Tu)
- `a2452f3d` Tweaks to buffered blockstore
  ([#1069](https://github.com/ChainSafe/forest/pull/1069)) (Austin Abell)
- `78303511` NetworkVersion 11 Upgrade at Epoch 665280
  ([#1066](https://github.com/ChainSafe/forest/pull/1066)) (Eric Tu)
- `b9fccde0` Release New Crates Due to Vulnerability in forest_message 0.6.0
  ([#1058](https://github.com/ChainSafe/forest/pull/1058)) (Eric Tu)
- `4a9e4e47` Reduce time to resolve links in flush to ~51ms in buffer blockstore
  writes ([#1059](https://github.com/ChainSafe/forest/pull/1059)) (creativcoder)
- `b3ad6d7a` Update bls_signatures to 0.9 and filecoin-proofs-api to 6.1
  ([#1062](https://github.com/ChainSafe/forest/pull/1062)) (Eric Tu)
- `7301c6bf` Fix Error Handling in Deadline Construction
  ([#1063](https://github.com/ChainSafe/forest/pull/1063)) (Eric Tu)
- `425ec083` Clippy "Fix"
  ([#1064](https://github.com/ChainSafe/forest/pull/1064)) (Eric Tu)
- `275f312e` HTTP RPC-JSON and tide-websockets
  ([#990](https://github.com/ChainSafe/forest/pull/990)) (Hunter Trujillo)
- `cfeab68f` Fix ExitCode handling when calling
  repay_partial_debt_in_priority_order
  ([#1055](https://github.com/ChainSafe/forest/pull/1055)) (Eric Tu)
- `d30d093d` Setup rustfmt
  ([#1053](https://github.com/ChainSafe/forest/pull/1053)) (Jorge Olivero)
- `d4fc556f` Libp2p Connection Limits
  ([#1051](https://github.com/ChainSafe/forest/pull/1051)) (Eric Tu)
- `36aaf693` Add @olibero to Code Owners
  ([#1052](https://github.com/ChainSafe/forest/pull/1052)) (Eric Tu)
- `8278d257` Fix ExitCode Handling in load_deadline
  ([#1050](https://github.com/ChainSafe/forest/pull/1050)) (Eric Tu)
- `77080e61` BufferedBlockstore Flush Improvements
  ([#1044](https://github.com/ChainSafe/forest/pull/1044)) (Eric Tu)
- `0bd9b1ef` Update Networking Log Levels
  ([#1046](https://github.com/ChainSafe/forest/pull/1046)) (Eric Tu)
- `669d5504` Fix Parent Grinding Fault Detection
  ([#1045](https://github.com/ChainSafe/forest/pull/1045)) (Eric Tu)
- `c424b65f` Fix ReportConsensusFault Gas Mismatch
  ([#1043](https://github.com/ChainSafe/forest/pull/1043)) (Eric Tu)
- `b2141ff3` Update Actors Interface and Fix Actors Consensus Issues
  ([#1041](https://github.com/ChainSafe/forest/pull/1041)) (Eric Tu)
- `0ddec266` Cargo Audit Patch
  ([#1042](https://github.com/ChainSafe/forest/pull/1042)) (Eric Tu)
- `f89b9ad1` Paych Actor v3
  ([#1035](https://github.com/ChainSafe/forest/pull/1035)) (Eric Tu)
- `608c0a93` Miner Actor v3
  ([#1032](https://github.com/ChainSafe/forest/pull/1032)) (Eric Tu)
- `01ae4250` Remove dutter and add creativ
  ([#1036](https://github.com/ChainSafe/forest/pull/1036)) (Eric Tu)
- `c4143e0a` Initial refactor: separate pool and provider
  ([#1027](https://github.com/ChainSafe/forest/pull/1027)) (creativcoder)
- `f6eddd54` Update libp2p to 0.35
  ([#928](https://github.com/ChainSafe/forest/pull/928)) (Austin Abell)
- `0b63a93f` Reward Actor v3
  ([#1020](https://github.com/ChainSafe/forest/pull/1020)) (Eric Tu)
- `caf19b94` Init Actor v3
  ([#1019](https://github.com/ChainSafe/forest/pull/1019)) (Eric Tu)
- `4a00c91e` Remove old codeowners
  ([#1018](https://github.com/ChainSafe/forest/pull/1018)) (Austin Abell)
- `49220a1d` Storage Power Actor v3
  ([#1017](https://github.com/ChainSafe/forest/pull/1017)) (Eric Tu)
- `91ba65b3` Update verifreg to v3
  ([#1016](https://github.com/ChainSafe/forest/pull/1016)) (Eric Tu)
- `4d663116` Document node and cleanup
  ([#1007](https://github.com/ChainSafe/forest/pull/1007)) (Austin Abell)
- `79c0da79` Multisig Actor v3
  ([#1013](https://github.com/ChainSafe/forest/pull/1013)) (Eric Tu)
- `0289e349` Market Actor v3
  ([#1010](https://github.com/ChainSafe/forest/pull/1010)) (Eric Tu)
- `4d0c8642` Update types documentation
  ([#1008](https://github.com/ChainSafe/forest/pull/1008)) (Austin Abell)
- `b42e66b5` Fix new clippy warnings and fix logic
  ([#1011](https://github.com/ChainSafe/forest/pull/1011)) (Austin Abell)
- `44dd57b8` Update VM docs.
  ([#1009](https://github.com/ChainSafe/forest/pull/1009)) (Austin Abell)
- `f20740ee` V3 HAMT and AMT for Actors
  ([#1005](https://github.com/ChainSafe/forest/pull/1005)) (Eric Tu)
- `bfb406b9` Update ipld docs
  ([#1004](https://github.com/ChainSafe/forest/pull/1004)) (Austin Abell)
- `69e91fc2` Update crypto docs and cleanup API
  ([#1002](https://github.com/ChainSafe/forest/pull/1002)) (Austin Abell)
- `9be446b5` Blockchain docs and cleanup
  ([#1000](https://github.com/ChainSafe/forest/pull/1000)) (Austin Abell)
- `121bdede` Actors v3 Setup
  ([#1001](https://github.com/ChainSafe/forest/pull/1001)) (Eric Tu)
- `e2034f74` Release Actors V2
  ([#994](https://github.com/ChainSafe/forest/pull/994)) (Eric Tu)
- `47b8b4f7` Add 1.46.0 msrv check to CI
  ([#993](https://github.com/ChainSafe/forest/pull/993)) (Austin Abell)
- `d38a08d5` Framework for State Migrations
  ([#987](https://github.com/ChainSafe/forest/pull/987)) (Eric Tu)
- `83906046` Improve Car file read node
  ([#988](https://github.com/ChainSafe/forest/pull/988)) (Austin Abell)
- `e642c55f` Fix hyper vulnerability
  ([#991](https://github.com/ChainSafe/forest/pull/991)) (Austin Abell)
- `bd546119` Include git hash and crate version
  ([#977](https://github.com/ChainSafe/forest/pull/977)) (Rajarupan Sampanthan)
- `46f7bf61` V3 Hamt update
  ([#982](https://github.com/ChainSafe/forest/pull/982)) (Austin Abell)
- `11d059ab` Fix bug in miner extend sector expiration
  ([#989](https://github.com/ChainSafe/forest/pull/989)) (Austin Abell)
- `84296d92` Nightly build/audit workaround and nightly linting fixes
  ([#983](https://github.com/ChainSafe/forest/pull/983)) (Austin Abell)
- `d1b2f622` Prep hamt v2 release
  ([#981](https://github.com/ChainSafe/forest/pull/981)) (Austin Abell)
- `c5342907` Fix fork handling
  ([#980](https://github.com/ChainSafe/forest/pull/980)) (Eric Tu)
- `931de226` V3 Actors Amt update
  ([#978](https://github.com/ChainSafe/forest/pull/978)) (Austin Abell)
- `4c2e4a07` Update Amt API and prep release
  ([#979](https://github.com/ChainSafe/forest/pull/979)) (Austin Abell)
- `d0a46ba7` Refactor discovery, improve Hello handling/peer management
  ([#975](https://github.com/ChainSafe/forest/pull/975)) (Austin Abell)
- `7ba2217d` Replace broadcast channels and refactor websocket streaming
  ([#955](https://github.com/ChainSafe/forest/pull/955)) (Austin Abell)
- `75403b30` Fix verify consensus fault logic
  ([#973](https://github.com/ChainSafe/forest/pull/973)) (Austin Abell)
- `76797645` Replace lazycell with once_cell
  ([#976](https://github.com/ChainSafe/forest/pull/976)) (Austin Abell)
- `7a8cce81` Fix block header signature verification helper function
  ([#972](https://github.com/ChainSafe/forest/pull/972)) (Austin Abell)
- `f182e8d6` Fix fork ([#971](https://github.com/ChainSafe/forest/pull/971))
  (Eric Tu)
- `34e1b1e6` Remove irrelevant spam logs
  ([#969](https://github.com/ChainSafe/forest/pull/969)) (Austin Abell)
- `c565eb92` Fix edge case in update pending deal state
  ([#968](https://github.com/ChainSafe/forest/pull/968)) (Austin Abell)
- `db0c4417` Fixes to ChainSyncer Scheduling
  ([#965](https://github.com/ChainSafe/forest/pull/965)) (Eric Tu)
- `3b16f807` Purge approvals on multisig removal
  ([#967](https://github.com/ChainSafe/forest/pull/967)) (Austin Abell)
- `3d91af03` Cleanup TODOs
  ([#933](https://github.com/ChainSafe/forest/pull/933)) (Austin Abell)
- `d7b9b396` Update logging to hide internal messages by default
  ([#954](https://github.com/ChainSafe/forest/pull/954)) (Austin Abell)
- `d82e3791` Interopnet support
  ([#964](https://github.com/ChainSafe/forest/pull/964)) (Austin Abell)
- `59c4413c` Add skip-load flag and cleanup snapshot loading
  ([#939](https://github.com/ChainSafe/forest/pull/939)) (Austin Abell)
- `ce37d70d` Fix to address resolution for chained internal calls
  ([#952](https://github.com/ChainSafe/forest/pull/952)) (Austin Abell)
- `18535f7c` Fix storage deal resolution pattern
  ([#948](https://github.com/ChainSafe/forest/pull/948)) (Austin Abell)
- `34b8c7eb` Update statediff for v2 and keep under feature
  ([#949](https://github.com/ChainSafe/forest/pull/949)) (Austin Abell)
- `3eed3ac2` Update bls backend to blst
  ([#945](https://github.com/ChainSafe/forest/pull/945)) (Austin Abell)
- `c02944fb` Handle null multisig proposal hashes
  ([#953](https://github.com/ChainSafe/forest/pull/953)) (Austin Abell)
- `5845f9e7` Ignore invalid peer id in miner info
  ([#951](https://github.com/ChainSafe/forest/pull/951)) (Austin Abell)
- `721bc466` Fix bugs in terminate sectors logic
  ([#950](https://github.com/ChainSafe/forest/pull/950)) (Austin Abell)
- `ccd4fbd0` Nullable Entropy in Randomness RPC and Fix Gas Base Fee Estimation
  ([#947](https://github.com/ChainSafe/forest/pull/947)) (Eric Tu)
- `4825a6a8` Fix calico vesting
  ([#942](https://github.com/ChainSafe/forest/pull/942)) (Austin Abell)
- `60e59697` Fix bug with pledge delta from proving deadline
  ([#943](https://github.com/ChainSafe/forest/pull/943)) (Austin Abell)
- `334551e1` Update consensus min power
  ([#946](https://github.com/ChainSafe/forest/pull/946)) (Austin Abell)
- `0e4ff578` Update calico storage gas multiplier
  ([#940](https://github.com/ChainSafe/forest/pull/940)) (Austin Abell)
- `53d35f02` Fix typo in v2 winning post validation
  ([#941](https://github.com/ChainSafe/forest/pull/941)) (Austin Abell)
- `749303c4` CBOR Stream Read in LibP2P RPC
  ([#932](https://github.com/ChainSafe/forest/pull/932)) (Eric Tu)
- `a79a97a9` Fix header signing bytes cache bug
  ([#935](https://github.com/ChainSafe/forest/pull/935)) (Austin Abell)
- `a32e19d3` Update header caches and builder
  ([#930](https://github.com/ChainSafe/forest/pull/930)) (Austin Abell)
- `ae9d7acb` Switch temp bytes deserialize to Cow
  ([#931](https://github.com/ChainSafe/forest/pull/931)) (Austin Abell)
- `a2ac9552` Clean up chain exchange responses and peer disconnects
  ([#929](https://github.com/ChainSafe/forest/pull/929)) (Austin Abell)
- `2f87782c` Update libp2p, async-std, and other deps
  ([#922](https://github.com/ChainSafe/forest/pull/922)) (Austin Abell)
- `9a6c9a87` Change default address prefix to 'f'
  ([#921](https://github.com/ChainSafe/forest/pull/921)) (Rajarupan Sampanthan)
- `182eacce` Fix ChainNotify RPC
  ([#924](https://github.com/ChainSafe/forest/pull/924)) (Eric Tu)
- `b97451a8` Update price list for calico VM gas
  ([#918](https://github.com/ChainSafe/forest/pull/918)) (Austin Abell)
- `67dfe7b1` Update calico vesting and refactor circ supply
  ([#917](https://github.com/ChainSafe/forest/pull/917)) (Austin Abell)
- `8efd8ee6` Claus fork burn removal
  ([#920](https://github.com/ChainSafe/forest/pull/920)) (Austin Abell)
- `ef3ecd1e` Calico (V7) update
  ([#919](https://github.com/ChainSafe/forest/pull/919)) (Austin Abell)
- `3c536d0d` Setup multiple network configurations and drand schedule
  ([#915](https://github.com/ChainSafe/forest/pull/915)) (Austin Abell)
- `fa82654f` Adding Insecure Post-Validation
  ([#916](https://github.com/ChainSafe/forest/pull/916)) (Rajarupan Sampanthan)
- `05b282da` Wrap ChainSyncer::state in an Arc<Mutex> and set it to Follow
  accordingly ([#914](https://github.com/ChainSafe/forest/pull/914)) (Tim
  Vermeulen)
- `adfbe855` Update smoke base fee calculation
  ([#912](https://github.com/ChainSafe/forest/pull/912)) (Austin Abell)
- `dd173e41` Update randomness for conformance tipsets and update V7 proof
  verification ([#911](https://github.com/ChainSafe/forest/pull/911)) (Austin
  Abell)
- `4b23b65c` Update types for all new V2 network upgrades
  ([#906](https://github.com/ChainSafe/forest/pull/906)) (Austin Abell)
- `0da265de` Replace shared actor crates with local code
  ([#907](https://github.com/ChainSafe/forest/pull/907)) (Austin Abell)
- `c604ae5f` Update miner actor to v2
  ([#903](https://github.com/ChainSafe/forest/pull/903)) (Austin Abell)
- `e4e39094` Update reward actor to v2
  ([#897](https://github.com/ChainSafe/forest/pull/897)) (Austin Abell)
- `62325dd7` Add GetMarketState to State Manager
  ([#900](https://github.com/ChainSafe/forest/pull/900)) (Ayush Mishra)
- `208c7655` Update runtime for network version upgrades
  ([#896](https://github.com/ChainSafe/forest/pull/896)) (Austin Abell)
- `c1a7553f` Storage Miner Pledging
  ([#899](https://github.com/ChainSafe/forest/pull/899)) (Eric Tu)
- `d6af098e` Add serde annotation for go vec visitor without using wrapper
  ([#898](https://github.com/ChainSafe/forest/pull/898)) (Austin Abell)
- `56f6d628` Update verifreg actor to v2
  ([#889](https://github.com/ChainSafe/forest/pull/889)) (Austin Abell)
- `88d1d465` Add a cfg flag to build Forest in Devnet mode
  ([#895](https://github.com/ChainSafe/forest/pull/895)) (Eric Tu)
- `8a1f2038` Paych v2 actor update
  ([#894](https://github.com/ChainSafe/forest/pull/894)) (Austin Abell)
- `f821b971` Power actor v2 upgrade
  ([#893](https://github.com/ChainSafe/forest/pull/893)) (Austin Abell)
- `d1dbd0ab` Add syncing configuration options
  ([#892](https://github.com/ChainSafe/forest/pull/892)) (Dustin Brickwood)
- `83a86a9d` Handle pubsub blocks in parallel
  ([#891](https://github.com/ChainSafe/forest/pull/891)) (Tim Vermeulen)
- `5dee4491` Update seal proof types and proofs api version
  ([#890](https://github.com/ChainSafe/forest/pull/890)) (Austin Abell)
- `169f9e3f` Version Hamt, Amt, State tree
  ([#887](https://github.com/ChainSafe/forest/pull/887)) (Austin Abell)
- `ecc7c680` Update market actor to v2
  ([#888](https://github.com/ChainSafe/forest/pull/888)) (Austin Abell)
- `dcbac0f0` Update multisig to v2
  ([#886](https://github.com/ChainSafe/forest/pull/886)) (Austin Abell)
- `5043ed0c` Update CI build checks
  ([#885](https://github.com/ChainSafe/forest/pull/885)) (Austin Abell)
- `787dcc0c` Implement Net API Module + Some other RPC methods
  ([#884](https://github.com/ChainSafe/forest/pull/884)) (Eric Tu)
- `47f2bd1e` Actors v2 upgrade setup
  ([#854](https://github.com/ChainSafe/forest/pull/854)) (Austin Abell)
- `aa448702` Storage Miner Init Interop
  ([#882](https://github.com/ChainSafe/forest/pull/882)) (Eric Tu)
- `8e821e0b` Remove coverage from CI until fixed
  ([#883](https://github.com/ChainSafe/forest/pull/883)) (Austin Abell)
- `61e5465e` Fix message crate compilation with json feature
  ([#880](https://github.com/ChainSafe/forest/pull/880)) (Austin Abell)
- `85e96837` Release crates for actors v2 upgrade
  ([#879](https://github.com/ChainSafe/forest/pull/879)) (Austin Abell)
- `82ca74cc` Form tipsets ([#875](https://github.com/ChainSafe/forest/pull/875))
  (Tim Vermeulen)
- `ce5c28b9` A bunch of fixes for the RPC
  ([#874](https://github.com/ChainSafe/forest/pull/874)) (Eric Tu)
- `8193a4b1` Implement hamt fuzzer
  ([#872](https://github.com/ChainSafe/forest/pull/872)) (Austin Abell)
- `36154ff6` Implement Amt fuzzer
  ([#871](https://github.com/ChainSafe/forest/pull/871)) (Austin Abell)
- `15c0e67f` Fix syncing regressions from #841
  ([#873](https://github.com/ChainSafe/forest/pull/873)) (Austin Abell)
- `6aaa037a` Update message json format to match Lotus
  ([#870](https://github.com/ChainSafe/forest/pull/870)) (Austin Abell)
- `56b4961f` Rpc fixes and implementations
  ([#841](https://github.com/ChainSafe/forest/pull/841)) (Purple Hair Rust Bard)
- `1da3294c` Fork serde_bytes to disallow string and array deserialization
  ([#868](https://github.com/ChainSafe/forest/pull/868)) (Austin Abell)
- `44f2c22e` Fix exit code handling in market
  ([#866](https://github.com/ChainSafe/forest/pull/866)) (Austin Abell)
- `f325bb35` Fix unlock unvested funds
  ([#865](https://github.com/ChainSafe/forest/pull/865)) (Austin Abell)
- `d690d484` Implement chain export functionality and car writer
  ([#861](https://github.com/ChainSafe/forest/pull/861)) (Austin Abell)
- `1b4dd61d` Add sled backend and cleanup RocksDb type
  ([#858](https://github.com/ChainSafe/forest/pull/858)) (Austin Abell)
- `db642466` MessagePool Greedy Message Selection
  ([#856](https://github.com/ChainSafe/forest/pull/856)) (Eric Tu)
- `26082703` Implement chain index
  ([#855](https://github.com/ChainSafe/forest/pull/855)) (Austin Abell)
- `c300f8e4` Update dependencies
  ([#859](https://github.com/ChainSafe/forest/pull/859)) (Dustin Brickwood)
- `d6c9bf60` Allow importing snapshot from URL + Progress bar
  ([#762](https://github.com/ChainSafe/forest/pull/762))
  ([#811](https://github.com/ChainSafe/forest/pull/811)) (Stepan)
- `178d1679` Setup mocking panic handling in vm
  ([#857](https://github.com/ChainSafe/forest/pull/857)) (Austin Abell)
- `60d12063` Switch param deserialization to the runtime to interop
  ([#853](https://github.com/ChainSafe/forest/pull/853)) (Austin Abell)
- `f9249b15` Use upstream cid
  ([#850](https://github.com/ChainSafe/forest/pull/850)) (Volker Mische)
- `85786ad3` Update CODEOWNERS
  ([#848](https://github.com/ChainSafe/forest/pull/848)) (Austin Abell)
- `0c1febbf` Rename BlockSync -> ChainExchange and tweak parameters
  ([#852](https://github.com/ChainSafe/forest/pull/852)) (Austin Abell)
- `876b9881` Refactor ChainStore, tipset usage, cache loaded tipsets
  ([#851](https://github.com/ChainSafe/forest/pull/851)) (Austin Abell)
- `396052ca` Fix: update balance table
  ([#849](https://github.com/ChainSafe/forest/pull/849)) (Austin Abell)
- `af28ef40` Update amt for_each caching mechanisms and usages
  ([#847](https://github.com/ChainSafe/forest/pull/847)) (Austin Abell)
- `ee25ba92` Add Networking and Republishing logic to MsgPool
  ([#732](https://github.com/ChainSafe/forest/pull/732)) (Eric Tu)
- `ef2583db` Use concrete implementations
  ([#842](https://github.com/ChainSafe/forest/pull/842)) (Volker Mische)
- `3c8a57b7` Fix gossipsub handling to process only when in follow state
  ([#845](https://github.com/ChainSafe/forest/pull/845)) (Austin Abell)
- `c53a5b82` Fix bug with import and cleanup
  ([#844](https://github.com/ChainSafe/forest/pull/844)) (Austin Abell)
- `1832a05e` Fix makefile test commands
  ([#843](https://github.com/ChainSafe/forest/pull/843)) (Austin Abell)
- `293ef19e` Fixes code cov build
  ([#840](https://github.com/ChainSafe/forest/pull/840)) (Dustin Brickwood)
- `62ea7ef4` Update multihash dependency and make Cid impl Copy
  ([#839](https://github.com/ChainSafe/forest/pull/839)) (Austin Abell)
- `cb1e2c41` Update README.md with security policy
  ([#831](https://github.com/ChainSafe/forest/pull/831)) (Amer Ameen)
- `d7b76cc9` Fix circleCI coverage
  ([#836](https://github.com/ChainSafe/forest/pull/836)) (Austin Abell)
- `e51e1ece` Fix allow internal reset after failed tx
  ([#834](https://github.com/ChainSafe/forest/pull/834)) (Austin Abell)
- `a86f0056` CircleCI updates, removal of github actions
  ([#813](https://github.com/ChainSafe/forest/pull/813)) (Dustin Brickwood)
- `d74b34ee` Add Gossipsub chain messages to MPool in the ChainSyncer instead of
  Libp2p Service ([#833](https://github.com/ChainSafe/forest/pull/833)) (Eric
  Tu)
- `bbdddf9d` Fix block messages generation for sequence edge case
  ([#832](https://github.com/ChainSafe/forest/pull/832)) (Austin Abell)
- `e1f1244b` Refactor chainstore and related components
  ([#809](https://github.com/ChainSafe/forest/pull/809)) (Austin Abell)
- `7990d4d4` Cleanup batch_verify_seal and tipset state execution
  ([#807](https://github.com/ChainSafe/forest/pull/807)) (Austin Abell)
- `0b7a49b6` Process blocks coming off of GossipSub
  ([#808](https://github.com/ChainSafe/forest/pull/808)) (Eric Tu)
- `1a447316` Defer bitfield decoding until its first use
  ([#803](https://github.com/ChainSafe/forest/pull/803)) (Tim Vermeulen)
- `ede37134` Update proofs and other deps
  ([#812](https://github.com/ChainSafe/forest/pull/812)) (Austin Abell)
- `a97f4b3b` Fix message receipt json
  ([#810](https://github.com/ChainSafe/forest/pull/810)) (Austin Abell)
- `b31fa0ae` Update gossip messagepool error log
  ([#805](https://github.com/ChainSafe/forest/pull/805)) (Austin Abell)
- `3bec812c` Fix unsealed sector padding
  ([#804](https://github.com/ChainSafe/forest/pull/804)) (Austin Abell)
- `9eab4a8a` Fix reschedule sector expirations
  ([#802](https://github.com/ChainSafe/forest/pull/802)) (Austin Abell)
- `21d6108f` Optimize statediff and fix hamt bug
  ([#799](https://github.com/ChainSafe/forest/pull/799)) (Austin Abell)
- `7da52345` Add msgs to msg pool
  ([#797](https://github.com/ChainSafe/forest/pull/797)) (Dustin Brickwood)
- `901d00cf` Increase cron gas limit
  ([#796](https://github.com/ChainSafe/forest/pull/796)) (Austin Abell)
- `ca3c131f` Import snapshots and import chains
  ([#789](https://github.com/ChainSafe/forest/pull/789)) (Eric Tu)
- `7d34126d` Refactor types out of actors crate to prep for v2 upgrade
  ([#790](https://github.com/ChainSafe/forest/pull/790)) (Austin Abell)
- `54635c52` Fix Miner cron related things
  ([#795](https://github.com/ChainSafe/forest/pull/795)) (Austin Abell)
- `27d3e668` GossipSub Message and Block Deserialization
  ([#791](https://github.com/ChainSafe/forest/pull/791)) (Eric Tu)
- `34710f7e` Update Dockerfile and include make command
  ([#792](https://github.com/ChainSafe/forest/pull/792)) (Dustin Brickwood)
- `5a8dfaab` Switch store for randomness retrieval
  ([#793](https://github.com/ChainSafe/forest/pull/793)) (Austin Abell)
- `b94f39de` Update bootnodes
  ([#794](https://github.com/ChainSafe/forest/pull/794)) (Austin Abell)
- `2e1bb096` Adding CircSupply Calculations
  ([#710](https://github.com/ChainSafe/forest/pull/710)) (nannick)
- `36e38c67` Wrap cached state in async mutex to avoid duplicate state
  calculation ([#785](https://github.com/ChainSafe/forest/pull/785)) (Austin
  Abell)
- `20de7a45` Adding Block Probability Calculations
  ([#771](https://github.com/ChainSafe/forest/pull/771)) (nannick)
- `bee59904` Fix deferred cron and validate caller
  ([#782](https://github.com/ChainSafe/forest/pull/782)) (Austin Abell)
- `78e6bb4b` Put Hamt reordering fix under a feature
  ([#783](https://github.com/ChainSafe/forest/pull/783)) (Austin Abell)
- `7743da7e` Fix projection period for faults
  ([#784](https://github.com/ChainSafe/forest/pull/784)) (Austin Abell)
- `fb2ca2be` Build and Api Versoining
  ([#752](https://github.com/ChainSafe/forest/pull/752)) (Purple Hair Rust Bard)
- `aa397491` Fix get_sectors_for_winning_post and cleanup
  ([#781](https://github.com/ChainSafe/forest/pull/781)) (Austin Abell)
- `dd707577` Fix provider and block message limit
  ([#780](https://github.com/ChainSafe/forest/pull/780)) (Austin Abell)
- `199d7feb` Refactor statediff to own crate
  ([#779](https://github.com/ChainSafe/forest/pull/779)) (Austin Abell)
- `6fb284a2` Fix sync messages logic
  ([#778](https://github.com/ChainSafe/forest/pull/778)) (Austin Abell)
- `c571e0e0` Fix tipset sorting function
  ([#777](https://github.com/ChainSafe/forest/pull/777)) (Austin Abell)
- `f9ab4c20` Update sequence based on epoch for internal messages
  ([#775](https://github.com/ChainSafe/forest/pull/775)) (Austin Abell)
- `057ff0d4` Fix chaos actor send
  ([#773](https://github.com/ChainSafe/forest/pull/773)) (Austin Abell)
- `0834808a` Minimum power fix
  ([#774](https://github.com/ChainSafe/forest/pull/774)) (Eric Tu)
- `34759a88` Fix Amt iter mut guard
  ([#772](https://github.com/ChainSafe/forest/pull/772)) (Austin Abell)
- `655eac22` Update conformance vectors
  ([#768](https://github.com/ChainSafe/forest/pull/768)) (Austin Abell)
- `1de532f3` Fix link in README
  ([#770](https://github.com/ChainSafe/forest/pull/770)) (Austin Abell)
- `6535480f` Remove protoc dependency and update README
  ([#769](https://github.com/ChainSafe/forest/pull/769)) (Austin Abell)
- `fc05e125` Minor fixes found during devnet testing
  ([#753](https://github.com/ChainSafe/forest/pull/753)) (Purple Hair Rust Bard)
- `982738c2` Authorization Setup for Write Access on RPC Calls
  ([#620](https://github.com/ChainSafe/forest/pull/620)) (Jaden Foldesi)
- `d4de5481` Fix blockstore get gas charge
  ([#751](https://github.com/ChainSafe/forest/pull/751)) (Austin Abell)
- `fd89bb9f` Implement actor upgrade logic
  ([#750](https://github.com/ChainSafe/forest/pull/750)) (Austin Abell)
- `6a46d7a4` Add a `ValueMut` wrapper type that tracks whether AMT values are
  mutated ([#749](https://github.com/ChainSafe/forest/pull/749)) (Tim Vermeulen)
- `0a7e163e` Fix CronEvents in Miner Actor
  ([#748](https://github.com/ChainSafe/forest/pull/748)) (Eric Tu)
- `3e8fd6cf` Fix block validations and add chain export test
  ([#741](https://github.com/ChainSafe/forest/pull/741)) (Austin Abell)
- `a4e399d9` Remove extra state load from withdraw balance
  ([#747](https://github.com/ChainSafe/forest/pull/747)) (Austin Abell)
- `15e00e80` Fix trailing 0s on bitfield serialization
  ([#746](https://github.com/ChainSafe/forest/pull/746)) (Austin Abell)
- `7e7d79c3` Switch serde_cbor to fork to allow unsafe unchecked utf8
  deserialization ([#745](https://github.com/ChainSafe/forest/pull/745)) (Austin
  Abell)
- `f8e16554` Fix Balance Table get
  ([#744](https://github.com/ChainSafe/forest/pull/744)) (Eric Tu)
- `6f187420` Skip send in transfer_to_actor if value is 0
  ([#743](https://github.com/ChainSafe/forest/pull/743)) (Eric Tu)
- `9dbcc47d` MAX_MINER_PROVE_COMMITS_PER_EPOCH 3 to 200
  ([#742](https://github.com/ChainSafe/forest/pull/742)) (Eric Tu)
- `cecf8713` State diff on root mismatch option
  ([#738](https://github.com/ChainSafe/forest/pull/738)) (Austin Abell)
- `f309a28b` Fix miner constructor params and proving period offset calculation
  ([#740](https://github.com/ChainSafe/forest/pull/740)) (Austin Abell)
- `891cd3ce` Fix base fee in conformance, bump versions and cleanup
  ([#739](https://github.com/ChainSafe/forest/pull/739)) (Austin Abell)
- `1b23fa33` Add label to DealProposal
  ([#737](https://github.com/ChainSafe/forest/pull/737)) (Austin Abell)
- `a75b47ac` Add redundant writes to Hamt and Amt to interop
  ([#731](https://github.com/ChainSafe/forest/pull/731)) (Austin Abell)
- `d910cf02` Update conformance vectors
  ([#734](https://github.com/ChainSafe/forest/pull/734)) (Austin Abell)
- `9de9e351` BlockSync provider
  ([#724](https://github.com/ChainSafe/forest/pull/724)) (Stepan)
- `0d86e686` Update params and fetch on daemon start
  ([#733](https://github.com/ChainSafe/forest/pull/733)) (Austin Abell)
- `cb4f779b` Updating Chaos Actor and Test Vectors
  ([#696](https://github.com/ChainSafe/forest/pull/696)) (nannick)
- `15eb3f07` Fix signature verification in mempool
  ([#727](https://github.com/ChainSafe/forest/pull/727)) (Austin Abell)
- `3ec23378` Update proof verification
  ([#726](https://github.com/ChainSafe/forest/pull/726)) (Austin Abell)
- `82f76ae6` Add caching to Hamt and update testing
  ([#730](https://github.com/ChainSafe/forest/pull/730)) (Austin Abell)
- `630b54f1` Update Actors error handling
  ([#722](https://github.com/ChainSafe/forest/pull/722)) (Austin Abell)
- `e59b7ae6` Switch bigint division usage to Euclidean to match Go
  ([#723](https://github.com/ChainSafe/forest/pull/723)) (Austin Abell)
- `2561c51c` Amt refactor and interop tests
  ([#716](https://github.com/ChainSafe/forest/pull/716)) (Austin Abell)
- `7a471d57` Remove unnecessary feature with audit failure
  ([#721](https://github.com/ChainSafe/forest/pull/721)) (Austin Abell)
- `b073565e` Mempool Update
  ([#705](https://github.com/ChainSafe/forest/pull/705)) (Eric Tu)
- `f0072101` Update Miner actor
  ([#691](https://github.com/ChainSafe/forest/pull/691)) (Tim Vermeulen)
- `285b9c34` Storage miner integration
  ([#670](https://github.com/ChainSafe/forest/pull/670)) (Purple Hair Rust Bard)
- `04274b14` Adding More Reward Actor Tests
  ([#715](https://github.com/ChainSafe/forest/pull/715)) (nannick)
- `dae9342f` Update block validations
  ([#711](https://github.com/ChainSafe/forest/pull/711)) (Austin Abell)
- `6dc3b50d` Update AMT max index bound
  ([#714](https://github.com/ChainSafe/forest/pull/714)) (Austin Abell)
- `d4dee5a2` Make block validations async
  ([#702](https://github.com/ChainSafe/forest/pull/702)) (Austin Abell)
- `8ef5ae5c` Peer stats tracking and selection
  ([#701](https://github.com/ChainSafe/forest/pull/701)) (Austin Abell)
- `dc0ff4cd` Semantic Validation for Messages
  ([#703](https://github.com/ChainSafe/forest/pull/703)) (Eric Tu)
- `3411459d` ChainSync refactor
  ([#693](https://github.com/ChainSafe/forest/pull/693)) (Austin Abell)
- `66ca99e2` Fix StateManager use in different components
  ([#694](https://github.com/ChainSafe/forest/pull/694)) (Eric Tu)
- `96b64cb2` Drand ignore env variable
  ([#697](https://github.com/ChainSafe/forest/pull/697)) (nannick)
- `548a4645` Print out conformance results and add log for skips
  ([#695](https://github.com/ChainSafe/forest/pull/695)) (Austin Abell)
- `0d7b16cc` Add CLI command to add Genesis Miner to Genesis Template
  ([#644](https://github.com/ChainSafe/forest/pull/644)) (Stepan)
- `0be6b76a` Chain syncing verification fixes
  ([#503](https://github.com/ChainSafe/forest/pull/503)) (Eric Tu)
- `156b2fb6` Fix docs publish workflow
  ([#688](https://github.com/ChainSafe/forest/pull/688)) (Austin Abell)
- `b54d0ec7` Update statetree cache
  ([#668](https://github.com/ChainSafe/forest/pull/668)) (Dustin Brickwood)
- `0809097f` Update blocksync message formats
  ([#686](https://github.com/ChainSafe/forest/pull/686)) (Austin Abell)
- `0db7ddbb` Tipset vector runner
  ([#682](https://github.com/ChainSafe/forest/pull/682)) (Austin Abell)
- `41ad3220` Swap secio authentication for noise
  ([#685](https://github.com/ChainSafe/forest/pull/685)) (Austin Abell)
- `93faacde` Fix bitswap breaking patch release
  ([#683](https://github.com/ChainSafe/forest/pull/683)) (Austin Abell)
- `f2c3ff0f` Update apply_blocks call
  ([#678](https://github.com/ChainSafe/forest/pull/678)) (Austin Abell)
- `e411eeed` Remove TODOs from scoping
  ([#675](https://github.com/ChainSafe/forest/pull/675)) (Austin Abell)
- `24341646` Adding Mdns and Kad Toggle
  ([#647](https://github.com/ChainSafe/forest/pull/647)) (nannick)
- `26517fba` Update edge cases for dynamic error handling in VM
  ([#671](https://github.com/ChainSafe/forest/pull/671)) (Austin Abell)
- `cd68b539` Update runtime transaction logic
  ([#666](https://github.com/ChainSafe/forest/pull/666)) (Austin Abell)
- `d5ccf900` Update EPOCH_DURATION_SECONDS
  ([#667](https://github.com/ChainSafe/forest/pull/667)) (Eric Tu)
- `c73394bc` Fix serialization of TxnIdParams
  ([#665](https://github.com/ChainSafe/forest/pull/665)) (Eric Tu)
- `7b4174e3` Fix runtime implementation to return gas blockstore
  ([#664](https://github.com/ChainSafe/forest/pull/664)) (Austin Abell)
- `2712508e` Handle failed retrieve of actor state
  ([#663](https://github.com/ChainSafe/forest/pull/663)) (Eric Tu)
- `bbbfacba` Check value correctly before transfer
  ([#662](https://github.com/ChainSafe/forest/pull/662)) (Eric Tu)
- `8d87c681` Add validation in chaos actor
  ([#661](https://github.com/ChainSafe/forest/pull/661)) (Eric Tu)
- `7d2bd2fa` Fix caller validation on nested sends
  ([#660](https://github.com/ChainSafe/forest/pull/660)) (Austin Abell)
- `5db465c3` Make hamt value type generic and add benchmarks
  ([#635](https://github.com/ChainSafe/forest/pull/635)) (Austin Abell)
- `5e35560c` Fix internal send bug, remove message ref from runtime
  ([#659](https://github.com/ChainSafe/forest/pull/659)) (Austin Abell)
- `ff754b90` Fix Get Actor
  ([#658](https://github.com/ChainSafe/forest/pull/658)) (Eric Tu)
- `0d82f424` Fix bugs in vm and update runner
  ([#657](https://github.com/ChainSafe/forest/pull/657)) (Austin Abell)
- `809e3e8c` Allow registering of Actors to the VM
  ([#654](https://github.com/ChainSafe/forest/pull/654)) (Eric Tu)
- `0f8185b2` Fix inconsistencies in apply_message
  ([#656](https://github.com/ChainSafe/forest/pull/656)) (Austin Abell)
- `4e0efe99` Add benchmarks and cleanup AMT
  ([#626](https://github.com/ChainSafe/forest/pull/626)) (Austin Abell)
- `aa6167c8` Expose message fields
  ([#655](https://github.com/ChainSafe/forest/pull/655)) (Austin Abell)
- `fd911984` Adding Chaos Actor
  ([#653](https://github.com/ChainSafe/forest/pull/653)) (nannick)
- `d0bd5844` Fix actor creation and deletion logic
  ([#652](https://github.com/ChainSafe/forest/pull/652)) (Austin Abell)
- `81557c9e` Space race genesis and bootnodes updates
  ([#650](https://github.com/ChainSafe/forest/pull/650)) (Austin Abell)
- `f1bf6079` Released updated protocol crates
  ([#651](https://github.com/ChainSafe/forest/pull/651)) (Austin Abell)
- `3fe1d46a` Update gas charges in VM
  ([#649](https://github.com/ChainSafe/forest/pull/649)) (Austin Abell)
- `0fb2fa38` Adding Puppet Actor
  ([#627](https://github.com/ChainSafe/forest/pull/627)) (nannick)
- `3e6c2ee7` Conformance test runner
  ([#638](https://github.com/ChainSafe/forest/pull/638)) (Austin Abell)
- `a4171bec` Builtin actors 0.9.3 update
  ([#643](https://github.com/ChainSafe/forest/pull/643)) (Austin Abell)
- `6029b716` Update libp2p, proofs, and other deps
  ([#641](https://github.com/ChainSafe/forest/pull/641)) (Austin Abell)
- `bbd20ccf` Dynamic Gas Implementation
  ([#639](https://github.com/ChainSafe/forest/pull/639)) (Eric Tu)
- `12ea58cf` Power actor update
  ([#621](https://github.com/ChainSafe/forest/pull/621)) (Austin Abell)
- `23718156` Separate ticket and beacon randomness
  ([#637](https://github.com/ChainSafe/forest/pull/637)) (Austin Abell)
- `da57abae` Update commcid to new codes and validation
  ([#601](https://github.com/ChainSafe/forest/pull/601)) (Austin Abell)
- `2b54a873` Update default hamt hash function to sha256 and make algo generic
  ([#624](https://github.com/ChainSafe/forest/pull/624)) (Austin Abell)
- `f0c0149a` Update header serialization
  ([#636](https://github.com/ChainSafe/forest/pull/636)) (Austin Abell)
- `22971156` Update to new empty amt serialization
  ([#623](https://github.com/ChainSafe/forest/pull/623)) (Austin Abell)
- `0a7036b0` Rpc state implementation
  ([#618](https://github.com/ChainSafe/forest/pull/618)) (Purple Hair Rust Bard)
- `336ae3b5` Add Bitfield cut operator and other improvements
  ([#617](https://github.com/ChainSafe/forest/pull/617)) (Tim Vermeulen)
- `88fdfbc0` Update system actor
  ([#622](https://github.com/ChainSafe/forest/pull/622)) (Austin Abell)
- `bbbb9e2b` New Genesis Template cli command
  ([#612](https://github.com/ChainSafe/forest/pull/612)) (Stepan)
- `8192c126` Fix bug in reading and persisting keystore data
  ([#625](https://github.com/ChainSafe/forest/pull/625)) (Austin Abell)
- `1b43ad5e` Reward actor update
  ([#619](https://github.com/ChainSafe/forest/pull/619)) (Austin Abell)
- `208f1719` Update verified registry actor
  ([#609](https://github.com/ChainSafe/forest/pull/609)) (Austin Abell)
- `87aec324` Add Persistent KeyStore
  ([#604](https://github.com/ChainSafe/forest/pull/604)) (Jaden Foldesi)
- `b6602bbd` Fix string decode handling network
  ([#611](https://github.com/ChainSafe/forest/pull/611)) (Austin Abell)
- `4aeadf71` Paych actor updates
  ([#608](https://github.com/ChainSafe/forest/pull/608)) (Austin Abell)
- `9c8e9a60` Smoothing Functions For Actors
  ([#594](https://github.com/ChainSafe/forest/pull/594)) (nannick)
- `a84ac2f0` Multisig actor update
  ([#606](https://github.com/ChainSafe/forest/pull/606)) (Austin Abell)
- `90353963` Market actor update
  ([#593](https://github.com/ChainSafe/forest/pull/593)) (Austin Abell)
- `44c1cd9b` Add address bugfix tests and bump crate version
  ([#598](https://github.com/ChainSafe/forest/pull/598)) (Austin Abell)
- `d7dcaf6e` Remove incorrectly ported sanity check from go implementation
  ([#597](https://github.com/ChainSafe/forest/pull/597)) (Austin Abell)
- `8b0b35cd` Returns an Error in case of slicing non-ascii strings
  ([#599](https://github.com/ChainSafe/forest/pull/599)) (Natanael Mojica)
- `52a5acec` Update Drand to use HTTP with the new endpoint
  ([#591](https://github.com/ChainSafe/forest/pull/591)) (Eric Tu)
- `4783a670` Update cron actor
  ([#588](https://github.com/ChainSafe/forest/pull/588)) (Austin Abell)
- `c642d9a9` JSON client setup and chain CLI commands
  ([#572](https://github.com/ChainSafe/forest/pull/572)) (Dustin Brickwood)
- `f80cfab7` Update account actor and params defaults/checks
  ([#587](https://github.com/ChainSafe/forest/pull/587)) (Austin Abell)
- `25203cb9` Add bulk put blockstore function and update header persisting
  ([#570](https://github.com/ChainSafe/forest/pull/570)) (Austin Abell)
- `9982c586` Add Drand Beacon Cache
  ([#586](https://github.com/ChainSafe/forest/pull/586)) (Stepan)
- `0c0b617c` Update init actor
  ([#589](https://github.com/ChainSafe/forest/pull/589)) (Austin Abell)
- `2863f64e` VM and Runtime updates
  ([#569](https://github.com/ChainSafe/forest/pull/569)) (Austin Abell)
- `fc523a32` Message Pool RPC
  ([#551](https://github.com/ChainSafe/forest/pull/551)) (Jaden Foldesi)
- `907eda8f` State API - RPC methods
  ([#532](https://github.com/ChainSafe/forest/pull/532)) (Purple Hair Rust Bard)
- `7cb6cecd` Add actor error convenience macro
  ([#550](https://github.com/ChainSafe/forest/pull/550)) (Austin Abell)
- `b9ae7e8f` Switch to use MessageInfo and refactor MockRuntime
  ([#552](https://github.com/ChainSafe/forest/pull/552)) (Austin Abell)
- `53378a9f` Test Runner for Message Signing Serialization Vectors
  ([#548](https://github.com/ChainSafe/forest/pull/548)) (Jaden Foldesi)
- `d3a1776c` Wallet rpc ([#512](https://github.com/ChainSafe/forest/pull/512))
  (Jaden Foldesi)
- `916bd4a6` Have BitField store ranges instead of bytes, and add benchmarks
  ([#543](https://github.com/ChainSafe/forest/pull/543)) (Tim Vermeulen)
- `37617d38` Send events through publisher
  ([#549](https://github.com/ChainSafe/forest/pull/549)) (Eric Tu)
- `0af8cfba` Update machine version for docs publish
  ([#546](https://github.com/ChainSafe/forest/pull/546)) (Austin Abell)
- `6c30ffe5` TokenAmount and StoragePower to BigInt
  ([#540](https://github.com/ChainSafe/forest/pull/540)) (nannick)
- `d83dac3c` Bitswap Integration
  ([#518](https://github.com/ChainSafe/forest/pull/518)) (Eric Tu)
- `b635c087` Implement Mock Runtime Syscalls
  ([#542](https://github.com/ChainSafe/forest/pull/542)) (nannick)
- `1b04f1ca` Implement Sync API and improve syncing
  ([#539](https://github.com/ChainSafe/forest/pull/539)) (Austin Abell)
- `9c561287` Paych actor tests
  ([#492](https://github.com/ChainSafe/forest/pull/492)) (nannick)
- `c7e94f24` Implement msg pool
  ([#449](https://github.com/ChainSafe/forest/pull/449)) (Jaden Foldesi)
- `1c58f7a4` Move libp2p from fork and bump versions
  ([#534](https://github.com/ChainSafe/forest/pull/534)) (Austin Abell)
- `41256318` Adding RPC Configuration
  ([#531](https://github.com/ChainSafe/forest/pull/531)) (nannick)
- `250ad1bf` Implements deadline tests and chain epoch update to i64
  ([#533](https://github.com/ChainSafe/forest/pull/533)) (Dustin Brickwood)
- `0facf1ba` Refactor RPC and network events
  ([#530](https://github.com/ChainSafe/forest/pull/530)) (Austin Abell)
- `2d88d06c` Remove bitvec dependency and other bit field changes
  ([#525](https://github.com/ChainSafe/forest/pull/525)) (Tim Vermeulen)
- `e0d574e1` Update async-std runtime setup
  ([#526](https://github.com/ChainSafe/forest/pull/526)) (Austin Abell)
- `a2cab731` Implementing Market Balance
  ([#524](https://github.com/ChainSafe/forest/pull/524)) (nannick)
- `7143e42b` Refactor CLI and implement fetch-params
  ([#516](https://github.com/ChainSafe/forest/pull/516)) (Austin Abell)
- `95a2fcc1` Update proofs-api to 4.0.1
  ([#523](https://github.com/ChainSafe/forest/pull/523)) (Austin Abell)
- `6e33c231` Bitfield improvements
  ([#506](https://github.com/ChainSafe/forest/pull/506)) (Tim Vermeulen)
- `8de380d2` Rupan/market actor tests
  ([#426](https://github.com/ChainSafe/forest/pull/426)) (nannick)
- `68c026ae` Fix docs push rule
  ([#520](https://github.com/ChainSafe/forest/pull/520)) (Austin Abell)
- `f68ae5dd` Update default branch name to main
  ([#519](https://github.com/ChainSafe/forest/pull/519)) (Austin Abell)
- `2650f8e8` Remove dead code and update CI
  ([#517](https://github.com/ChainSafe/forest/pull/517)) (Austin Abell)
- `4422cddc` A bare-bones GraphSync ResponseManager
  ([#511](https://github.com/ChainSafe/forest/pull/511)) (Tim Vermeulen)
- `11411a37` Update dependencies and proofs version
  ([#515](https://github.com/ChainSafe/forest/pull/515)) (Austin Abell)
- `8add7840` Update bootnodes and genesis for testnet
  ([#509](https://github.com/ChainSafe/forest/pull/509)) (Austin Abell)
- `1ff34dbc` Update proofs to v4
  ([#507](https://github.com/ChainSafe/forest/pull/507)) (Austin Abell)
- `0f1dba04` Updates market actor
  ([#496](https://github.com/ChainSafe/forest/pull/496)) (Dustin Brickwood)
- `18f6aacc` Implement Kademlia discovery
  ([#501](https://github.com/ChainSafe/forest/pull/501)) (Austin Abell)
- `dd396b9f` Fix ecrecover and verify methods
  ([#500](https://github.com/ChainSafe/forest/pull/500)) (Jaden Foldesi)
- `a326fec9` Ashanti/winning posts
  ([#493](https://github.com/ChainSafe/forest/pull/493)) (Purple Hair Rust Bard)
- `f66b4e1a` Added ctrl addresses
  ([#494](https://github.com/ChainSafe/forest/pull/494)) (Dustin Brickwood)
- `f38a0016` Miner actor implemented
  ([#486](https://github.com/ChainSafe/forest/pull/486)) (Dustin Brickwood)
- `8d2a4936` Implement Wallet
  ([#469](https://github.com/ChainSafe/forest/pull/469)) (Jaden Foldesi)
- `418c2fed` Adds Json serialization for Message Receipts and ActorState
  ([#484](https://github.com/ChainSafe/forest/pull/484)) (Eric Tu)
- `a49f9a96` Update Reward actor to new spec
  ([#480](https://github.com/ChainSafe/forest/pull/480)) (Austin Abell)
- `33310f57` Implements Storage Miner critical Chain API methods
  ([#478](https://github.com/ChainSafe/forest/pull/478)) (Eric Tu)
- `21b3b498` Batch seal verification implementation
  ([#483](https://github.com/ChainSafe/forest/pull/483)) (Austin Abell)
- `78e32a99` Switch bls pub key from using vec
  ([#481](https://github.com/ChainSafe/forest/pull/481)) (Austin Abell)
- `afb1fc82` Updated Code to Pass Checks With Rust 1.44
  ([#479](https://github.com/ChainSafe/forest/pull/479)) (Jaden Foldesi)
- `9cc24d70` Tide JSONRPC over HTTP
  ([#462](https://github.com/ChainSafe/forest/pull/462)) (Eric Tu)
- `7e0540f1` Ashanti/chain store channel
  ([#473](https://github.com/ChainSafe/forest/pull/473)) (Purple Hair Rust Bard)
- `2ad2399b` Ashanti/connect state transition
  ([#454](https://github.com/ChainSafe/forest/pull/454)) (Purple Hair Rust Bard)
- `a0dbd7bd` Implement Block header json
  ([#470](https://github.com/ChainSafe/forest/pull/470)) (Austin Abell)
- `206ec565` Update power, reward and market actors, rt and registered proofs
  relative to miner actor ([#458](https://github.com/ChainSafe/forest/pull/458))
  (Dustin Brickwood)
- `63083135` Implement compatible bitfield
  ([#466](https://github.com/ChainSafe/forest/pull/466)) (Austin Abell)
- `6dab2336` Add CircleCI ([#441](https://github.com/ChainSafe/forest/pull/441))
  (Gregory Markou)
- `8ee51446` Add PeerResponseSender
  ([#453](https://github.com/ChainSafe/forest/pull/453)) (Tim Vermeulen)
- `f4b306d6` Update dockerfiles for protoc install
  ([#460](https://github.com/ChainSafe/forest/pull/460)) (Austin Abell)
- `fb1b4085` Bump async-std to 1.6
  ([#456](https://github.com/ChainSafe/forest/pull/456)) (Eric Tu)
- `fc34b441` Runtime randomness and ChainStore randomness
  ([#415](https://github.com/ChainSafe/forest/pull/415)) (Eric Tu)
- `47835025` Fix block header serialization
  ([#450](https://github.com/ChainSafe/forest/pull/450)) (Austin Abell)
- `91f44e20` Setup GraphSync network interface
  ([#442](https://github.com/ChainSafe/forest/pull/442)) (Austin Abell)
- `129f17e0` Bump versions for release
  ([#451](https://github.com/ChainSafe/forest/pull/451)) (Austin Abell)
- `078eda17` Signed and Unsigned message json impls
  ([#444](https://github.com/ChainSafe/forest/pull/444)) (Austin Abell)
- `bb982fbb` Update libp2p to 0.19
  ([#439](https://github.com/ChainSafe/forest/pull/439)) (Austin Abell)
- `acedcf08` Remove a bajillion manual serde implementations
  ([#433](https://github.com/ChainSafe/forest/pull/433)) (Tim Vermeulen)
- `c9a089e8` Update serialization vectors
  ([#435](https://github.com/ChainSafe/forest/pull/435)) (Austin Abell)
- `9ef6e67a` Add Secp address sanity check
  ([#438](https://github.com/ChainSafe/forest/pull/438)) (Austin Abell)
- `c58aeb4b` Setup GraphSync message types and protobuf encoding
  ([#434](https://github.com/ChainSafe/forest/pull/434)) (Austin Abell)
- `ba8d9467` Verifying Drand Entries from Blocks
  ([#387](https://github.com/ChainSafe/forest/pull/387)) (Eric Tu)
- `b1903ba2` Jaden/chainstore refactor
  ([#432](https://github.com/ChainSafe/forest/pull/432)) (Jaden Foldesi)
- `b80ab49d` Jaden/chainsync/asyncverification
  ([#419](https://github.com/ChainSafe/forest/pull/419)) (Jaden Foldesi)
- `033bade8` Update prefix bytes encoding to include mh len
  ([#431](https://github.com/ChainSafe/forest/pull/431)) (Austin Abell)
- `a04ba8f1` Cargo changes for publishing core crates
  ([#425](https://github.com/ChainSafe/forest/pull/425)) (Austin Abell)
- `42fc42e9` Add default implementations for Store bulk operations
  ([#424](https://github.com/ChainSafe/forest/pull/424)) (Tim Vermeulen)
- `ff39f2aa` Last block info for selectors
  ([#418](https://github.com/ChainSafe/forest/pull/418)) (Austin Abell)
- `1f96e918` Update docs and use rocksdb as feature
  ([#421](https://github.com/ChainSafe/forest/pull/421)) (Austin Abell)
- `187ca6cc` Remove Address default implementation
  ([#422](https://github.com/ChainSafe/forest/pull/422)) (Austin Abell)
- `932dae3b` Implemented verify post
  ([#416](https://github.com/ChainSafe/forest/pull/416)) (Purple Hair Rust Bard)
- `b7f6f92a` Shared types refactor
  ([#417](https://github.com/ChainSafe/forest/pull/417)) (Austin Abell)
- `6241454a` Implement Verify Registry Actor
  ([#413](https://github.com/ChainSafe/forest/pull/413)) (Purple Hair Rust Bard)
- `1598ff20` Update tipset sorting
  ([#412](https://github.com/ChainSafe/forest/pull/412)) (Austin Abell)
- `4c95043e` Ipld selector traversals implementation
  ([#408](https://github.com/ChainSafe/forest/pull/408)) (Austin Abell)
- `fa12fff0` Jaden/tipsetconversions
  ([#404](https://github.com/ChainSafe/forest/pull/404)) (Jaden Foldesi)
- `f47b7579` SyncBucket cleanup
  ([#407](https://github.com/ChainSafe/forest/pull/407)) (Tim Vermeulen)
- `ef7aafdc` Async Block verification in ChainSync
  ([#409](https://github.com/ChainSafe/forest/pull/409)) (Eric Tu)
- `0bba0ecf` Fix CI for docs build
  ([#406](https://github.com/ChainSafe/forest/pull/406)) (Austin Abell)
- `81257dfd` Rupan/reward actor tests
  ([#403](https://github.com/ChainSafe/forest/pull/403)) (nannick)
- `5a0bf8d9` Implement interfacing with Drand over GRPC
  ([#375](https://github.com/ChainSafe/forest/pull/375)) (Eric Tu)
- `91a7e651` Selector explore implementation
  ([#402](https://github.com/ChainSafe/forest/pull/402)) (Austin Abell)
- `cbbd921a` Refactor with structops macro
  ([#401](https://github.com/ChainSafe/forest/pull/401)) (Purple Hair Rust Bard)
- `a048f250` Init test porting
  ([#394](https://github.com/ChainSafe/forest/pull/394)) (nannick)
- `67a25f9c` Clean up tipsets
  ([#397](https://github.com/ChainSafe/forest/pull/397)) (Tim Vermeulen)
- `4e4fa120` Update proofs api
  ([#400](https://github.com/ChainSafe/forest/pull/400)) (Austin Abell)
- `2052eb9f` Bump Dependencies
  ([#399](https://github.com/ChainSafe/forest/pull/399)) (Eric Tu)
- `c57cf419` Implement verify seal syscall
  ([#393](https://github.com/ChainSafe/forest/pull/393)) (Dustin Brickwood)
- `18d179b2` DAGJson support for Ipld
  ([#390](https://github.com/ChainSafe/forest/pull/390)) (Austin Abell)
- `84fb0e44` IPLD Selector framework with serialization
  ([#395](https://github.com/ChainSafe/forest/pull/395)) (Austin Abell)
- `3f8c6cf6` Temporary build fix for fil-proofs
  ([#396](https://github.com/ChainSafe/forest/pull/396)) (Austin Abell)
- `0b589d73` Interop updates and refactoring
  ([#388](https://github.com/ChainSafe/forest/pull/388)) (Austin Abell)
- `1aa2e64d` Implements verify consensus fault syscall and reorg vm crates
  ([#386](https://github.com/ChainSafe/forest/pull/386)) (Dustin Brickwood)
- `aa899c08` BlockHeader Update
  ([#385](https://github.com/ChainSafe/forest/pull/385)) (Eric Tu)
- `7e8124de` Refactor address crate
  ([#376](https://github.com/ChainSafe/forest/pull/376)) (Austin Abell)
- `14894298` Implement buffered blockstore cache
  ([#383](https://github.com/ChainSafe/forest/pull/383)) (Austin Abell)
- `5355acd0` Apply Blocks and refactor
  ([#374](https://github.com/ChainSafe/forest/pull/374)) (Austin Abell)
- `8bc201a3` Added forest img
  ([#378](https://github.com/ChainSafe/forest/pull/378)) (Dustin Brickwood)
- `16483531` Added bls aggregate sig check for block validation
  ([#371](https://github.com/ChainSafe/forest/pull/371)) (Dustin Brickwood)
- `3d5aeb2b` Stmgr retrieval methods + is_ticket_winner calc for block
  validation ([#369](https://github.com/ChainSafe/forest/pull/369)) (Dustin
  Brickwood)
- `cecd33d7` Load Genesis from CAR file
  ([#329](https://github.com/ChainSafe/forest/pull/329)) (Eric Tu)
- `29b45845` Compute unsealed sector CID syscall
  ([#360](https://github.com/ChainSafe/forest/pull/360)) (Austin Abell)
- `c27deaba` Setup dockerfiles and update CI
  ([#370](https://github.com/ChainSafe/forest/pull/370)) (Austin Abell)
- `68c4f9ae` Implement Weight method + st/gt for heaviest ts
  ([#359](https://github.com/ChainSafe/forest/pull/359)) (Dustin Brickwood)
- `4382a82e` Mock Runtime and tests for Account and Cron Actors
  ([#356](https://github.com/ChainSafe/forest/pull/356)) (Eric Tu)
- `ff5260a6` Update to new PoSt sector types
  ([#357](https://github.com/ChainSafe/forest/pull/357)) (Austin Abell)
- `8f0fc1d5` Commitment to cid conversions
  ([#358](https://github.com/ChainSafe/forest/pull/358)) (Austin Abell)
- `3798a4e8` Disallow default Cid and Address serialization
  ([#354](https://github.com/ChainSafe/forest/pull/354)) (Austin Abell)
- `dbee0e66` Implements apply_message and apply_implicit_message
  ([#353](https://github.com/ChainSafe/forest/pull/353)) (Dustin Brickwood)
- `1e6533a4` Implement verify signature syscall and cleanup
  ([#351](https://github.com/ChainSafe/forest/pull/351)) (Austin Abell)
- `d257f5cd` Blake2b syscall
  ([#352](https://github.com/ChainSafe/forest/pull/352)) (Austin Abell)
- `38825e7b` VM gas usage implementation and refactor
  ([#350](https://github.com/ChainSafe/forest/pull/350)) (Austin Abell)
- `8f8fd1e3` Connect remaining Actor invocations to runtime and cleanup
  ([#342](https://github.com/ChainSafe/forest/pull/342)) (Austin Abell)
- `4f0c6f7d` Market actor implementation
  ([#338](https://github.com/ChainSafe/forest/pull/338)) (Dustin Brickwood)
- `380dde3e` Update block header serialization
  ([#337](https://github.com/ChainSafe/forest/pull/337)) (Austin Abell)
- `f5845a0b` Refactor error handling
  ([#336](https://github.com/ChainSafe/forest/pull/336)) (Austin Abell)
- `3bc7d410` Key hashing compatibility
  ([#333](https://github.com/ChainSafe/forest/pull/333)) (Austin Abell)
- `28760209` Update peerid serialization in miner actor
  ([#335](https://github.com/ChainSafe/forest/pull/335)) (Austin Abell)
- `35f3c973` Reward Actor ([#318](https://github.com/ChainSafe/forest/pull/318))
  (Austin Abell)
- `ca7898f7` Move bigint serialization utils
  ([#331](https://github.com/ChainSafe/forest/pull/331)) (Austin Abell)
- `e0a996bd` Miner state ([#325](https://github.com/ChainSafe/forest/pull/325))
  (Austin Abell)
- `d30d396e` Market actor state
  ([#330](https://github.com/ChainSafe/forest/pull/330)) (Austin Abell)
- `5f81a1d6` Runtime Implementation
  ([#323](https://github.com/ChainSafe/forest/pull/323)) (Eric Tu)
- `4aacd72a` Initial chainsync process
  ([#293](https://github.com/ChainSafe/forest/pull/293)) (Dustin Brickwood)
- `e2157f57` Logging level config with RUST_LOG env
  variable([#328](https://github.com/ChainSafe/forest/pull/328)) (Austin Abell)
- `29b9e265` Libp2p and dependency update
  ([#326](https://github.com/ChainSafe/forest/pull/326)) (Austin Abell)
- `618cf4ab` Storage Power actor
  ([#308](https://github.com/ChainSafe/forest/pull/308)) (Austin Abell)
- `6d2bf0e0` Switch MethodNum and ActorID to aliases
  ([#317](https://github.com/ChainSafe/forest/pull/317)) (Austin Abell)
- `661f52aa` Change ChainEpoch and TokenAmount types
  ([#309](https://github.com/ChainSafe/forest/pull/309)) (Austin Abell)
- `7e8bd6f2` Payment channel actor
  ([#299](https://github.com/ChainSafe/forest/pull/299)) (Austin Abell)
- `c97033c0` Bitfield/ rle+ impl
  ([#296](https://github.com/ChainSafe/forest/pull/296)) (Austin Abell)
- `5473af59` SetMultimap implementation
  ([#292](https://github.com/ChainSafe/forest/pull/292)) (Austin Abell)
- `1ae3ba41` Improve actors serialization handling
  ([#297](https://github.com/ChainSafe/forest/pull/297)) (Austin Abell)
- `31738128` Refactor annotated serializations
  ([#295](https://github.com/ChainSafe/forest/pull/295)) (Austin Abell)
- `a5b1ab3c` Sector types ([#294](https://github.com/ChainSafe/forest/pull/294))
  (Austin Abell)
- `08d523b5` Implement multimap
  ([#290](https://github.com/ChainSafe/forest/pull/290)) (Austin Abell)
- `338ddf9d` Feat(vm): implement improved error handling
  ([#289](https://github.com/ChainSafe/forest/pull/289)) (Friedel Ziegelmayer)
- `9611818a` Feat(vm): implement system-actor
  ([#288](https://github.com/ChainSafe/forest/pull/288)) (Friedel Ziegelmayer)
- `76712559` Implement balance table
  ([#285](https://github.com/ChainSafe/forest/pull/285)) (Austin Abell)
- `f2027f03` Update message for type changes
  ([#286](https://github.com/ChainSafe/forest/pull/286)) (Austin Abell)
- `af7ad3a7` Multisig actor implementation
  ([#284](https://github.com/ChainSafe/forest/pull/284)) (Austin Abell)
- `e1200ea6` Cron actor implementation
  ([#281](https://github.com/ChainSafe/forest/pull/281)) (Austin Abell)
- `d7f01992` Move abi types and implement piece size
  ([#283](https://github.com/ChainSafe/forest/pull/283)) (Austin Abell)
- `7c8fb8cf` Init actor implementation
  ([#282](https://github.com/ChainSafe/forest/pull/282)) (Austin Abell)
- `5d829172` Clean actors and update to spec
  ([#279](https://github.com/ChainSafe/forest/pull/279)) (Austin Abell)
- `d7195739` Remove unnecessary db trait
  ([#274](https://github.com/ChainSafe/forest/pull/274)) (Austin Abell)
- `0018c22c` State tree full implementation and refactor IPLD
  ([#273](https://github.com/ChainSafe/forest/pull/273)) (Austin Abell)
- `125a9a24` Move CodeID and ActorState to vm crate
  ([#271](https://github.com/ChainSafe/forest/pull/271)) (Eric Tu)
- `1a5b619b` Update exit codes
  ([#270](https://github.com/ChainSafe/forest/pull/270)) (Eric Tu)
- `b337f7c7` Clear warnings for new nightly toolchain rule
  ([#259](https://github.com/ChainSafe/forest/pull/259)) (Austin Abell)
- `28cb8f54` Hamt implementation
  ([#255](https://github.com/ChainSafe/forest/pull/255)) (Austin Abell)
- `17a447e0` Update runtime to new spec/ impl
  ([#256](https://github.com/ChainSafe/forest/pull/256)) (Austin Abell)
- `0c451396` Read CAR Files
  ([#254](https://github.com/ChainSafe/forest/pull/254)) (Eric Tu)
- `bd9f5cf0` Update multihash dependency and Blockstore interface
  ([#253](https://github.com/ChainSafe/forest/pull/253)) (Austin Abell)
- `6aeee77a` Implement basic peer manager
  ([#252](https://github.com/ChainSafe/forest/pull/252)) (Austin Abell)
- `970d476f` Implement sync fork
  ([#248](https://github.com/ChainSafe/forest/pull/248)) (Dustin Brickwood)
- `a43df302` Connected blocksync requests and network polling thread
  ([#244](https://github.com/ChainSafe/forest/pull/244)) (Austin Abell)
- `7b5d8db0` Refactor RPC and implement hello protocol
  ([#246](https://github.com/ChainSafe/forest/pull/246)) (Austin Abell)
- `a1672031` Adding Verification Function for Aggregate BLS Signatures
  ([#240](https://github.com/ChainSafe/forest/pull/240)) (DragonMural)
- `8c924495` ChainSync framework
  ([#243](https://github.com/ChainSafe/forest/pull/243)) (Austin Abell)
- `5a18b496` Libp2p RPC protocol and Blocksync
  ([#229](https://github.com/ChainSafe/forest/pull/229)) (Eric Tu)
- `47dfb47c` Update multibase dependency for lowercase base32 support
  ([#239](https://github.com/ChainSafe/forest/pull/239)) (Austin Abell)
- `39a8d88e` Allow Address network prefix to be overriden for printing
  ([#233](https://github.com/ChainSafe/forest/pull/233)) (Austin Abell)
- `faa71386` Refactor SyncManager to have ownership over tipsets
  ([#238](https://github.com/ChainSafe/forest/pull/238)) (Austin Abell)
- `f242043e` Remove all clones and copies from serializations
  ([#234](https://github.com/ChainSafe/forest/pull/234)) (Austin Abell)
- `d87704f1` State Manager and initial miner retrieval methods
  ([#224](https://github.com/ChainSafe/forest/pull/224)) (Dustin Brickwood)
- `a65794ba` Setup Forest execution threads
  ([#236](https://github.com/ChainSafe/forest/pull/236)) (Austin Abell)
- `56e62302` Global async logging
  ([#232](https://github.com/ChainSafe/forest/pull/232)) (Eric Tu)
- `14115728` Fix cid serde feature reference
  ([#235](https://github.com/ChainSafe/forest/pull/235)) (Austin Abell)
- `b598ffca` Local bigint serialization
  ([#231](https://github.com/ChainSafe/forest/pull/231)) (Austin Abell)
- `1bd9a59c` Refactor blockstore and Cid for usage
  ([#230](https://github.com/ChainSafe/forest/pull/230)) (Austin Abell)
- `abe9be2e` Fix cbor serialization formats and cleanup
  ([#228](https://github.com/ChainSafe/forest/pull/228)) (Austin Abell)
- `59e2fc7c` Refactor keypair retrieval and saving
  ([#221](https://github.com/ChainSafe/forest/pull/221)) (Austin Abell)
- `67ebaae2` Initial Validation Checks - Message, Timestamp and Block Sig
  ([#219](https://github.com/ChainSafe/forest/pull/219)) (Dustin Brickwood)
- `fdf6c506` Entry point for app for organization
  ([#220](https://github.com/ChainSafe/forest/pull/220)) (Austin Abell)
- `b25b4066` Fix README build badge
  ([#218](https://github.com/ChainSafe/forest/pull/218)) (Austin Abell)
- `6d4304e2` Added Fetch and Load Methods
  ([#196](https://github.com/ChainSafe/forest/pull/196)) (Dustin Brickwood)
- `b41107d3` Clean crypto crate and interfaces with Signature types
  ([#214](https://github.com/ChainSafe/forest/pull/214)) (Austin Abell)
- `75224a6a` Add audit to CI
  ([#213](https://github.com/ChainSafe/forest/pull/213)) (Austin Abell)
- `477930db` Migration to Stable Futures and Network Refactor
  ([#209](https://github.com/ChainSafe/forest/pull/209)) (Eric Tu)
- `8b1b61ba` AMT implementation
  ([#197](https://github.com/ChainSafe/forest/pull/197)) (Austin Abell)
- `62beb1b2` CBOR encoding for BlockHeader
  ([#192](https://github.com/ChainSafe/forest/pull/192)) (Eric Tu)
- `3d6814c8` Updated markdown for readme and templates
  ([#208](https://github.com/ChainSafe/forest/pull/208)) (Dustin Brickwood)
- `15ce6d2c` Add MIT license to dual
  ([#204](https://github.com/ChainSafe/forest/pull/204)) (Austin Abell)
- `547e35b7` Updated readme
  ([#201](https://github.com/ChainSafe/forest/pull/201)) (Dustin Brickwood)
- `21635617` Updated link to include internal discord
  ([#200](https://github.com/ChainSafe/forest/pull/200)) (Dustin Brickwood)
- `f21765c4` Rename repo ([#199](https://github.com/ChainSafe/forest/pull/199))
  (Austin Abell)
- `957da7ed` Update README.md
  ([#198](https://github.com/ChainSafe/forest/pull/198)) (ChainSafe Systems)
- `2514c406` Sync & Store methods updated
  ([#193](https://github.com/ChainSafe/forest/pull/193)) (Dustin Brickwood)
- `f1eb515b` DagCBOR encoding and decoding for Tickets
  ([#190](https://github.com/ChainSafe/forest/pull/190)) (Eric Tu)
- `58f3e03a` Update BlockHeader weight to BigUint and DagCBOR encoding for
  TipsetKeys ([#191](https://github.com/ChainSafe/forest/pull/191)) (Eric Tu)
- `8755ec16` Refactor to remove ToCid trait
  ([#186](https://github.com/ChainSafe/forest/pull/186)) (Austin Abell)
- `ab99a6ec` Wrap Signature into a struct
  ([#184](https://github.com/ChainSafe/forest/pull/184)) (Eric Tu)
- `8018d246` Added templates and config
  ([#183](https://github.com/ChainSafe/forest/pull/183)) (Dustin Brickwood)
- `7a9fa80b` Basic Syncer and ChainStore methods
  ([#173](https://github.com/ChainSafe/forest/pull/173)) (Dustin Brickwood)
- `7e524b3c` UnsignedMessage cbor encoding
  ([#174](https://github.com/ChainSafe/forest/pull/174)) (Austin Abell)
- `925d2711` MessageParams update and refactor
  ([#175](https://github.com/ChainSafe/forest/pull/175)) (Austin Abell)
- `1d6dd985` Add missing fields for BlockHeader
  ([#177](https://github.com/ChainSafe/forest/pull/177)) (Eric Tu)
- `f025a9bb` Update changes in spec, updated docs, updated function signatures
  ([#171](https://github.com/ChainSafe/forest/pull/171)) (Austin Abell)
- `c3c6e052` Updated cid format and IPLD link to Cid type
  ([#172](https://github.com/ChainSafe/forest/pull/172)) (Austin Abell)
- `6d527a47` Update message types function signatures
  ([#170](https://github.com/ChainSafe/forest/pull/170)) (Austin Abell)
- `468846f9` Refactor Blockheader
  ([#169](https://github.com/ChainSafe/forest/pull/169)) (Austin Abell)
- `e70250db` Fix existing bug with multibase ToCid
  ([#167](https://github.com/ChainSafe/forest/pull/167)) (Austin Abell)
- `89a0e60e` Add CODEOWNERS
  ([#166](https://github.com/ChainSafe/forest/pull/166)) (Austin Abell)
- `ae6861e2` Switch from using dynamic pointers
  ([#154](https://github.com/ChainSafe/forest/pull/154)) (Austin Abell)
- `b5295e25` Implement and update Cbor encoding
  ([#157](https://github.com/ChainSafe/forest/pull/157)) (Austin Abell)
- `e998474f` Update address for network config, clean auxiliary stuff
  ([#145](https://github.com/ChainSafe/forest/pull/145)) (Austin Abell)
- `c9d3fbbd` Readme Updates
  ([#159](https://github.com/ChainSafe/forest/pull/159)) (David Ansermino)
- `1e99c0ca` Implemented block format
  ([#149](https://github.com/ChainSafe/forest/pull/149)) (Dustin Brickwood)
- `7e58d9bc` Docs index redirect
  ([#151](https://github.com/ChainSafe/forest/pull/151)) (Austin Abell)
- `a641beca` Update Cid references, bump serde_cbor version
  ([#155](https://github.com/ChainSafe/forest/pull/155)) (Austin Abell)
- `79374e42` Fix license script and add to CI
  ([#150](https://github.com/ChainSafe/forest/pull/150)) (Austin Abell)
- `8431cad8` Docs Cleanup ([#138](https://github.com/ChainSafe/forest/pull/138))
  (David Ansermino)
- `9d04393e` Implement memory db
  ([#137](https://github.com/ChainSafe/forest/pull/137)) (Austin Abell)
- `d35410ef` Implement Basic SyncManager
  ([#132](https://github.com/ChainSafe/forest/pull/132)) (Austin Abell)
- `1576674f` Update makefile
  ([#140](https://github.com/ChainSafe/forest/pull/140)) (Gregory Markou)
- `0d29569c` Created wrapper for Cid type
  ([#134](https://github.com/ChainSafe/forest/pull/134)) (Austin Abell)
- `eace8d81` Storage Power Actor framework
  ([#129](https://github.com/ChainSafe/forest/pull/129)) (Austin Abell)
- `ede60e7b` Naive DB + Rocksdb implemenation
  ([#125](https://github.com/ChainSafe/forest/pull/125)) (Gregory Markou)
- `957d0529` Implement BlockHeader builder pattern
  ([#124](https://github.com/ChainSafe/forest/pull/124)) (Austin Abell)
- `d745c60d` Switch License to Apache
  ([#139](https://github.com/ChainSafe/forest/pull/139)) (Gregory Markou)
- `cee77ac1` Update libp2p version to fix cargo issue
  ([#136](https://github.com/ChainSafe/forest/pull/136)) (Austin Abell)
- `2c63bc56` Add License and license script
  ([#123](https://github.com/ChainSafe/forest/pull/123)) (Gregory Markou)
- `0ab143c4` CI cleanup ([#122](https://github.com/ChainSafe/forest/pull/122))
  (Austin Abell)
- `e4363f1a` Implement TipIndex
  ([#113](https://github.com/ChainSafe/forest/pull/113)) (Dustin Brickwood)
- `c916eb4d` Update StateTree and implement cache
  ([#108](https://github.com/ChainSafe/forest/pull/108)) (Austin Abell)
- `4c9fbd88` MethodParameter usage and implementation in system actors
  ([#107](https://github.com/ChainSafe/forest/pull/107)) (Austin Abell)
- `af198d43` Basic VRF ([#104](https://github.com/ChainSafe/forest/pull/104))
  (David Ansermino)
- `6818dc5d` Fix linting issues
  ([#105](https://github.com/ChainSafe/forest/pull/105)) (Austin Abell)
- `2bb5c0ac` Remove ref keywords
  ([#99](https://github.com/ChainSafe/forest/pull/99)) (Austin Abell)
- `d9a35b51` Remove redundant CI
  ([#102](https://github.com/ChainSafe/forest/pull/102)) (Austin Abell)
- `1bd01685` MethodParams update and implementation
  ([#103](https://github.com/ChainSafe/forest/pull/103)) (Austin Abell)
- `2b8ee0eb` Updated blocks crate to reflect spec changes
  ([#86](https://github.com/ChainSafe/forest/pull/86)) (Dustin Brickwood)
- `efb3d8fc` State tree and interpreter framework
  ([#97](https://github.com/ChainSafe/forest/pull/97)) (Austin Abell)
- `792f1204` Reward System Actor framework
  ([#95](https://github.com/ChainSafe/forest/pull/95)) (Austin Abell)
- `e41786ce` Account System Actor framework
  ([#96](https://github.com/ChainSafe/forest/pull/96)) (Austin Abell)
- `51a17882` Updated ChainEpoch usages and clock crate
  ([#98](https://github.com/ChainSafe/forest/pull/98)) (Austin Abell)
- `de80b34a` Refactor Message type and vm packages
  ([#79](https://github.com/ChainSafe/forest/pull/79)) (Austin Abell)
- `e99d8b57` Add libp2p Identify protocol
  ([#94](https://github.com/ChainSafe/forest/pull/94)) (Eric Tu)
- `cb4576d4` Cleanup epoch time
  ([#92](https://github.com/ChainSafe/forest/pull/92)) (Gregory Markou)
- `62888071` Readme typo fix
  ([#93](https://github.com/ChainSafe/forest/pull/93)) (Dustin Brickwood)
- `6a2092b7` Persist networking keystore
  ([#90](https://github.com/ChainSafe/forest/pull/90)) (Gregory Markou)
- `db7d7fc4` Ec2/libp2p ping
  ([#91](https://github.com/ChainSafe/forest/pull/91)) (Eric Tu)
- `36acea44` Cron system actor
  ([#84](https://github.com/ChainSafe/forest/pull/84)) (Austin Abell)
- `db5dad03` Update libp2p dep
  ([#87](https://github.com/ChainSafe/forest/pull/87)) (Austin Abell)
- `93caa63c` Initial Structures for Message - Manager Communication
  ([#69](https://github.com/ChainSafe/forest/pull/69)) (Dustin Brickwood)
- `054f25d4` InitActor framework
  ([#76](https://github.com/ChainSafe/forest/pull/76)) (Austin Abell)
- `d75c8f2e` CLI cleanup ([#70](https://github.com/ChainSafe/forest/pull/70))
  (Gregory Markou)
- `bbea6130` Add config file parsing
  ([#60](https://github.com/ChainSafe/forest/pull/60)) (Gregory Markou)
- `d10a5460` Runtime trait and vm types
  ([#68](https://github.com/ChainSafe/forest/pull/68)) (Austin Abell)
- `ca0159f6` Implements basic actor type
  ([#61](https://github.com/ChainSafe/forest/pull/61)) (Austin Abell)
- `3438654d` Fix makefile clean and phony targets
  ([#62](https://github.com/ChainSafe/forest/pull/62)) (Austin Abell)
- `af33dd2b` Implements Address cbor encoding
  ([#59](https://github.com/ChainSafe/forest/pull/59)) (Austin Abell)
- `bf608808` Create Networking Service
  ([#49](https://github.com/ChainSafe/forest/pull/49)) (Eric Tu)
- `acb00bb0` Closes #51 - Add basic makefile
  ([#57](https://github.com/ChainSafe/forest/pull/57)) (Gregory Markou)
- `9fd58b8d` Add file reading and writing
  ([#54](https://github.com/ChainSafe/forest/pull/54)) (Gregory Markou)
- `9fba7d98` New Tipset w/ unit tests
  ([#56](https://github.com/ChainSafe/forest/pull/56)) (Dustin Brickwood)
- `f7772339` Encoding library and standardizing usage
  ([#48](https://github.com/ChainSafe/forest/pull/48)) (Austin Abell)
- `996900c4` Add an async logger
  ([#53](https://github.com/ChainSafe/forest/pull/53)) (Eric Tu)
- `fd534869` Remove unneeded types
  ([#47](https://github.com/ChainSafe/forest/pull/47)) (Austin Abell)
- `dc04a06f` Add message and messageReceipt to block
  ([#37](https://github.com/ChainSafe/forest/pull/37)) (Gregory Markou)
- `ff9b6757` Refactor crypto and address to libraries
  ([#40](https://github.com/ChainSafe/forest/pull/40)) (Austin Abell)
- `e85e6992` [VM] Implement basic message types and signing stubs
  ([#31](https://github.com/ChainSafe/forest/pull/31)) (Austin Abell)
- `62194eb7` [VM] Address module cleanup
  ([#32](https://github.com/ChainSafe/forest/pull/32)) (Austin Abell)
- `ae846751` Basic blockchain types and Tipset methods
  ([#28](https://github.com/ChainSafe/forest/pull/28)) (Dustin Brickwood)
- `fd667b8d` Basic clock interface
  ([#27](https://github.com/ChainSafe/forest/pull/27)) (Gregory Markou)
- `29c8b441` Add basic cli ([#25](https://github.com/ChainSafe/forest/pull/25))
  (Gregory Markou)
- `e79cc5f7` [VM] Address logic and code restructure
  ([#21](https://github.com/ChainSafe/forest/pull/21)) (Austin Abell)
- `dba3d3ed` Fix build and setup node binary and subsystem libs
  ([#1](https://github.com/ChainSafe/forest/pull/1)) (Austin Abell)
- `0443c1d4` Remove signed block (Eric Tu)
- `ea16ee42` Chain_sync stub (austinabell)
- `995aa6ee` Fixed fn naming (Dustin Brickwood)
- `2644a493` Added stubbed message pool.rs (Dustin Brickwood)
- `bc6cb2a8` Blocks refactor (Eric Tu)
- `c0af00aa` Merge branch 'master' of github.com:ec2/rust-filecoin (Eric Tu)
- `10868e19` More block stubs (Eric Tu)
- `5eaa11cb` Change how types are referenced externally (austinabell)
- `a37ee2d4` Merge branch 'master' of github.com:ec2/rust-filecoin (Eric Tu)
- `a927122c` Exported message types for use (austinabell)
- `e78e209c` Basic incomplete stubbing for block (Eric Tu)
- `235f00d5` Stubbed some vm (Eric Tu)
- `cb354a60` Fix gitignore (austinabell)
- `e8e71e10` Remove cargo lock (austinabell)
- `f3cc8430` Updated lockfile (austinabell)
- `83730efd` Set up subsystems (austinabell)
- `04b6d5bf` Set up vm and blockchain system binaries (austinabell)
- `ba604fa5` Executable project template (Eric Tu)
- `8344e2b8` Initial commit (Eric Tu)
