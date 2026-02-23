<!--

## A short guide to adding a changelog entry

- pick a section to which your change belongs in _Forest unreleased_,
- the entry should follow the format:

  `[#ISSUE_NO](link to the issue): <short description>`, for example:

  [#1234](https://github.com/chainsafe/forest/pull/1234): Add support for NV18

- if the change does not have an issue, use the PR number instead - the PR must
  have a detailed description of the change and its motivation. Consider
  creating a separate issue if the change is complex enough to warrant it,
- the changelog is not a place for the full description of the change, it should
  be a short summary of the change,
- if the change does not directly affect the user, it should not be included in
  the changelog - for example, refactoring of the codebase,
- review the entry to make sure it is correct and understandable and that it
  does not contain any typos,
- the entries should not contradict each other - if you add a new entry, ensure
  it is consistent with the existing entries.

-->

## Forest unreleased

### Breaking

### Added

- [#3715](https://github.com/ChainSafe/forest/issues/3715): Implemented parallel HTTP downloads for snapshots with 5 concurrent connections by default (configurable via `FOREST_DOWNLOAD_CONNECTIONS`), bringing significant performance improvements for snapshot downloads (on par with a manual `aria2c -x5`).

### Changed

### Removed

### Fixed

- [#6613](https://github.com/ChainSafe/forest/pull/6613): Fixed chain sync getting stuck when encountering time-travelling blocks by not marking the corresponding tipsets as permanently bad.

- [#6594](https://github.com/ChainSafe/forest/issues/6594): Added random GC delay to avoid a cluster of nodes run GC and reboot RPC services at the same time.

## Forest v0.32.1 "Malfoy"

This is a non-mandatory release for all node operators. It sets F3 initial power table on calibnet for late F3 participation and F3 data verification scenarios. It also includes new V2 RPC methods, a few bug fixes and `lotus-gateway` compatibility fixes.

### Breaking

### Added

- [#6590](https://github.com/ChainSafe/forest/pull/6590): Set F3 `InitialPowerTable` on calibnet.

- [#6524](https://github.com/ChainSafe/forest/pull/6524): Implemented `Filecoin.EthSendRawTransactionUntrusted` for API v2.

- [#6513](https://github.com/ChainSafe/forest/pull/6513): Enabled `Filecoin.EthNewFilter` for API v2.

### Changed

### Removed

### Fixed

- [#6577](https://github.com/ChainSafe/forest/issues/6577): Fixed `Filecoin.EthGetBalance` compatibility issue with Lotus Gateway.

- [#6551](https://github.com/ChainSafe/forest/pull/6551): Fixed `ErrExecutionReverted` JSONRPCError conversion error with Lotus Gateway.

## Forest v0.32.0 "Ember"

This is a non-mandatory release for all node operators. It resets F3 on calibnet, also includes new V2 RPC methods, a few bug fixes and `lotus-gateway` compatibility fixes.

### Breaking

- [#6475](https://github.com/ChainSafe/forest/pull/6475): Increased default JWT (generated via `Filecoin.AuthNew`) expiration time from 24 hours to 100 years to match Lotus behavior and ensure compatibility with clients like Curio.

- [#6392](https://github.com/ChainSafe/forest/pull/6392): Changed execution reverted error code from 11 to 3.

### Added

- [#6465](https://github.com/ChainSafe/forest/pull/6465): Implemented `Filecoin.EthGetBlockTransactionCountByNumber` for API v2.

- [#6466](https://github.com/ChainSafe/forest/pull/6466): Enabled `Filecoin.EthGetBlockTransactionCountByHash` for API v2.

- [#6469](https://github.com/ChainSafe/forest/pull/6469): Implemented `Filecoin.EthGetTransactionByBlockNumberAndIndex` for API v2.

- [#6451](https://github.com/ChainSafe/forest/pull/6451): Implemented `Filecoin.EthTraceBlock` for API v2.

- [#6489](https://github.com/ChainSafe/forest/pull/6489): Implemented `Filecoin.EthGetBlockReceipts` for API v2.

- [#6490](https://github.com/ChainSafe/forest/pull/6490): Implemented `Filecoin.EthGetCode` for API v2.

- [#6492](https://github.com/ChainSafe/forest/pull/6492): Implemented `Filecoin.EthGetStorageAt` for API v2.

- [#6498](https://github.com/ChainSafe/forest/pull/6498): Implemented `Filecoin.EthGetBlockReceiptsLimited` for API v2.

### Changed

- [#6471](https://github.com/ChainSafe/forest/pull/6471): Moved `forest-tool state` subcommand to `forest-dev`.

- [#6527](https://github.com/ChainSafe/forest/issues/6527): Increased the maximum number of allowed connections to the RPC server to 1000. This can be further configured via the `FOREST_RPC_MAX_CONNECTIONS` environment variable.

### Removed

### Fixed

- [#6467](https://github.com/ChainSafe/forest/pull/6467): `Filecoin.EthGetBlockByNumber` now only supports retrieving a block by its block number or a special tag.

- [#6531](https://github.com/ChainSafe/forest/issues/6531): `Filecoin.EthGetBlockByHash` now works with `lotus-gateway`.

- [#6552](https://github.com/ChainSafe/forest/issues/6552): `Filecoin.ChainGetTipset` now works with `lotus-gateway`.

- [#6535](https://github.com/ChainSafe/forest/pull/6535): Fixed incorrect replace by fee behavior when at limits of pending messages in mempool.

- [#6541](https://github.com/ChainSafe/forest/pull/6541): Fixed "actor not found" errors when running Foundry (forge) scripts. The `eth_getBalance`, `eth_getTransactionCount`, and `eth_getCode` methods now return default values (0 balance, 0 nonce, empty code) for non-existent addresses, matching Lotus and standard Ethereum behavior.

- [#6555](https://github.com/ChainSafe/forest/pull/6555): Fixed listing of wallets belonging to different networks in `forest-wallet list` command (and the `Filecoin.WalletList` RPC method). This incorrectly showed, e.g., calibnet wallets when running a mainnet node. Under the hood they're actually the same, but this could cause confusion and issues with some clients. It also resulted in errors trying to export a wallet that belongs to a different network.

## Forest v0.31.1 "Quadrantids"

This is a non-mandatory release for all node operators. It includes the support for more V2 API's and a few critical API fixes.

### Added

- [#6339](https://github.com/ChainSafe/forest/pull/6339) Implemented `Filecoin.EthCall` for API v2.

- [#6364](https://github.com/ChainSafe/forest/pull/6364) Implemented `Filecoin.EthEstimateGas` for API v2.

- [#6380](https://github.com/ChainSafe/forest/pull/6380) Implemented `Filecoin.EthFeeHistory` for API v2.

- [#6387](https://github.com/ChainSafe/forest/pull/6387) Implemented `Filecoin.EthGetTransactionCount` for API v2.

- [#6403](https://github.com/ChainSafe/forest/pull/6403) Implemented `Filecoin.EthGetBalance` for API v2.

- [#6404](https://github.com/ChainSafe/forest/pull/6404) Implemented `Filecoin.EthGetBlockByNumber` for API v2.

- [#6405](https://github.com/ChainSafe/forest/pull/6405) Enabled `Filecoin.EthGetLogs` for API v2.

- [#6421](https://github.com/ChainSafe/forest/pull/6421) Add an environment variable `FOREST_RPC_BACKFILL_FULL_TIPSET_FROM_NETWORK` to enable backfilling full tipsets from network in a few RPC methods.

- [#6444](https://github.com/ChainSafe/forest/pull/6444) Implemented `Filecoin.EthTraceReplayBlockTransactions` for API v2.

### Changed

- [#6368](https://github.com/ChainSafe/forest/pull/6368): Migrated build and development tooling from Makefile to `mise`. Contributors should install `mise` and use `mise run` commands instead of `make` commands.

- [#6286](https://github.com/ChainSafe/forest/pull/6286) `Filecoin.ChainGetEvents` now returns an error if the events are not present in the db.

- [#6444](https://github.com/ChainSafe/forest/pull/6444) `EthReplayBlockTransactionTrace` responses now always include `stateDiff` and `vmTrace` fields (set to `null` when not available) for Lotus compatibility.

### Fixed

- [#6409](https://github.com/ChainSafe/forest/pull/6409) Fixed backfill issue when null tipsets are present.

- [#6327](https://github.com/ChainSafe/forest/pull/6327) Fixed: Forest returns 404 for all invalid api paths.

- [#6354](https://github.com/ChainSafe/forest/pull/6354) Fixed: Correctly calculate the epoch range instead of directly using the look back limit value while searching for messages.

- [#6400](https://github.com/ChainSafe/forest/issues/6400) Fixed `eth_subscribe` `newHeads` to return Ethereum block format instead of Filecoin block headers array.

- [#6286](https://github.com/ChainSafe/forest/pull/6286) Fixed: `Filecoin.ChainGetEvents` API returns correct events.

- [#6430](https://github.com/ChainSafe/forest/issues/6430) Fixed a panic when syncing from genesis on the calibration network.

- [#6456](https://github.com/ChainSafe/forest/pull/6456) Whitelisted nebula and hermes crawlers.

## Forest v0.30.5 "Dulce de Leche"

Non-mandatory release supporting new API methods and addressing a critical panic issue.

### Added

- [#6231](https://github.com/ChainSafe/forest/pull/6231) Implemented `Filecoin.ChainGetTipSet` for API v2.

- [#6312](https://github.com/ChainSafe/forest/pull/6312) Implemented `Filecoin.StateGetActor` for API v2.

- [#6312](https://github.com/ChainSafe/forest/pull/6312) Implemented `Filecoin.StateGetID` for API v2.

- [#6323](https://github.com/ChainSafe/forest/pull/6323) Implemented `Filecoin.FilecoinAddressToEthAddress` for API v1 and v2.

### Fixed

- [#6325](https://github.com/ChainSafe/forest/pull/6325) Fixed a panic that could occur under certain message pool conditions and the `Filecoin.MpoolSelect` RPC method.

- [#5979](https://github.com/ChainSafe/forest/issues/5979) Fixed an issue with `Filecoin.EthGetCode` and `Filecoin.EthGetStorageAt` returning parent tipset data instead of the requested tipset.

- [#6118](https://github.com/ChainSafe/forest/pull/6118) Fixed the `Filecoin.EthGetTransactionReceipt` and `Filecoin.EthGetTransactionReceiptLimited` RPC methods to return null for non-existent transactions instead of an error. This aligns with the Ethereum RPC API provided by Lotus.

- [#6118](https://github.com/ChainSafe/forest/pull/6118) Removed a legacy limit of 100M gas for messages which was preventing contract deployments.

## Forest v0.30.4 "DeLorean"

This is a non-mandatory release that fixes a chain sync issue that is caused by time traveling block(s).

### Fixed

- [#6241](https://github.com/ChainSafe/forest/pull/6241) Fixed a sync issue that is caused by time traveling block(s).

## Forest v0.30.3 "Trishul"

This is a non-mandatory release that brings important enhancements in Forest's tooling capabilities.
The release includes new CLI commands for snapshot monitoring, a crucial fork handling bug fix and ETH API performance improvements, and error handling.

### Added

- [#6082](https://github.com/ChainSafe/forest/issues/6082) Added `forest-cli snapshot export-status` and `forest-cli snapshot export-cancel` subcommands to monitor or cancel an export, respectively.

- [#6161](https://github.com/ChainSafe/forest/pull/6161) Added `forest-tool db import` subcommand.

- [#6166](https://github.com/ChainSafe/forest/pull/6166) Gate `JWT` expiration validation behind environment variable `FOREST_JWT_DISABLE_EXP_VALIDATION`.

- [#6167](https://github.com/ChainSafe/forest/pull/6167) Added `forest-tool state compute` subcommand to generate database snapshot for tipset validation.

- [#6167](https://github.com/ChainSafe/forest/pull/6167) Added `forest-tool state replay-compute` subcommand to replay tipset validation with a minimal database snapshot.

- [#6171](https://github.com/ChainSafe/forest/pull/6171) Enable V2 API support for basic Eth RPC methods: `EthChainId`, `EthProtocolVersion`, `EthSyncing`, `EthAccounts`.

### Changed

- [#6145](https://github.com/ChainSafe/forest/pull/6145) Updated `forest-cli snapshot export` to use v2 format by default.

### Fixed

- [#6178](https://github.com/ChainSafe/forest/pull/6178) Fixed incorrect error code for unsupported RPC methods.

- [#6189](https://github.com/ChainSafe/forest/pull/6189) Improved performance of `eth_getBlockByHash` and `eth_getBlockByNumber`.

- [#6206](https://github.com/ChainSafe/forest/pull/6206) Fixed batch request error in RPC server.

- [#6234](https://github.com/ChainSafe/forest/pull/6234) Support `input` as an alias for `data` in `eth_call` and `eth_estimateGas` RPC methods.

- [#6235](https://github.com/ChainSafe/forest/pull/6235) Fixed a potential deadlock in chain follower when handling fork(s).

## Forest v0.30.2 "Garuda"

This is a non-mandatory release that brings important enhancements to Forest's tooling capabilities, Ethereum RPC compatibility, and F3 integration.
The release includes new CLI commands for snapshot management and state inspection, along with critical fixes for Ethereum RPC methods.

### Added

- [#6074](https://github.com/ChainSafe/forest/issues/6074) Added `forest-cli snapshot export-diff` subcommand for exporting a diff snapshot.

- [#6061](https://github.com/ChainSafe/forest/pull/6061) Added `forest-cli state actor-cids` command for listing all actor CIDs in the state tree for the current tipset.

- [#5568](https://github.com/ChainSafe/forest/issues/5568) Added `--n-tipsets` flag to the `forest-tool index backfill` subcommand to specify the number of epochs to backfill.

- [#6133](https://github.com/ChainSafe/forest/pull/6133) Added `Filecoin.ChainGetFinalizedTipset` API method to get the finalized tipset using f3.

### Fixed

- [#5055](https://github.com/ChainSafe/forest/issues/5055) Fixed an issue where Forest fails on duplicate drand entries. This would only happen on new devnets.

- [#6103](https://github.com/ChainSafe/forest/pull/6103) Fixed `eth_getTransactionCount` to return the nonce of the requested tipset and not its parent.

- [#6140](https://github.com/ChainSafe/forest/pull/6140) Fixed the `eth_getLogs` RPC method to accept `None` as the `address` parameter.

## Forest v0.30.1 "Laurelin"

Mandatory release for mainnet node operators. It sets the NV27 _Golden Week_ network upgrade to epoch `5_348_280` which corresponds to `Wed 24 Sep 23:00:00 UTC 2025`. It also includes a few improvements that help with snapshot generation and inspection.

### Added

- [#6057](https://github.com/ChainSafe/forest/issues/6057) Added `--no-progress-timeout` to `forest-cli f3 ready` subcommand to exit when F3 is stuck for the given timeout.

- [#6000](https://github.com/ChainSafe/forest/pull/6000) Added support for the `Filecoin.StateDecodeParams` API methods to enable decoding actors method params.

- [#6079](https://github.com/ChainSafe/forest/pull/6079) Added prometheus metrics `network_version`, `network_version_revision` and `actor_version`.

- [#6068](https://github.com/ChainSafe/forest/issues/6068) Added `--index-backfill-epochs` to `forest-tool api serve`.

### Changed

- [#6068](https://github.com/ChainSafe/forest/issues/6068) Made `--chain` optional in `forest-tool api serve` by inferring network from the snapshots.

## Forest v0.30.0 "Eärendil"

Mandatory release for calibration network node operators. It includes the NV27 _Golden Week_ network upgrade at epoch `3_007_294` which corresponds to `Wed 10 Sep 23:00:00 UTC 2025`. This release also includes a few breaking changes (removal of unused commands) and minor fixes.

### Added

- [#6006](https://github.com/ChainSafe/forest/issues/6006) More strict checks for the address arguments in the `forest-cli` subcommands.

- [#5897](https://github.com/ChainSafe/forest/issues/5987) Added support for the NV27 _Golden Week_ network upgrade for devnets.

- [#5897](https://github.com/ChainSafe/forest/issues/5987) Added support for the NV27 _Golden Week_ network upgrade for calibration network. The upgrade epoch is set to `3_007_294` (Wed 10 Sep 23:00:00 UTC 2025).

### Removed

- [#6010](https://github.com/ChainSafe/forest/pull/6010) Removed the deprecated `forest-cli send` subcommand. Use `forest-wallet send` instead.

- [#6014](https://github.com/ChainSafe/forest/pull/6014) Removed `--unordered` from `forest-cli snapshot export`.

- [#6014](https://github.com/ChainSafe/forest/pull/6014) Removed `unordered-graph-traversal` from `forest-tool benchmark`.

- [#6037](https://github.com/ChainSafe/forest/pull/6037) Removed `--track-peak-rss` from `forest` in favor of external tools like `/usr/bin/time -v`.

### Fixed

- [#6028](https://github.com/ChainSafe/forest/pull/6028) Fixed missing `Teep` and `Tock` network upgrade entries in `Filecoin.StateGetNetworkParams` RPC method.

## Forest v0.29.0 "Fëanor"

Non-mandatory release. It introduces a couple of features around snapshot generation and inspection. It fully supports the new FRC-0108 Filecoin snapshot format. There is also a notable fix in `Filecoin.ChainNotify` RPC method that would cause issues with some clients.

### Breaking

- [#5946](https://github.com/ChainSafe/forest/pull/5946) Updated parameters and response of `Forest.StateCompute` RPC method to support new `forest-cli state compute` options.

### Added

- [#5835](https://github.com/ChainSafe/forest/issues/5835) Add `--format` flag to the `forest-cli snapshot export` subcommand. This allows exporting a Filecoin snapshot in v2 format(FRC-0108).

- [#5956](https://github.com/ChainSafe/forest/pull/5956) Add `forest-tool archive f3-header` subcommand for inspecting the header of a standalone F3 snapshot(FRC-0108).

- [#5835](https://github.com/ChainSafe/forest/issues/5835) Add `forest-tool archive metadata` subcommand for inspecting snapshot metadata of a Filecoin snapshot in v2 format(FRC-0108).

- [#5859](https://github.com/ChainSafe/forest/pull/5859) Added size metrics for zstd frame cache and made max size configurable via `FOREST_ZSTD_FRAME_CACHE_DEFAULT_MAX_SIZE` environment variable.

- [#5963](https://github.com/ChainSafe/forest/pull/5963) Added `forest-cli f3 ready` command for checking whether F3 is in sync.

- [#5867](https://github.com/ChainSafe/forest/pull/5867) Added `--unordered` to `forest-cli snapshot export` for exporting `CAR` blocks in non-deterministic order for better performance with more parallelization.

- [#5946](https://github.com/ChainSafe/forest/pull/5946) Added `--n-epochs` to `forest-cli state compute` for computating state trees in batch.

- [#5946](https://github.com/ChainSafe/forest/pull/5946) Added `--verbose` to `forest-cli state compute` for printing epochs and tipset keys along with state roots.

- [#5886](https://github.com/ChainSafe/forest/issues/5886) Add `forest-tool archive merge-f3` subcommand for merging a v1 Filecoin snapshot and an F3 snapshot into a v2 Filecoin snapshot.

- [#4976](https://github.com/ChainSafe/forest/issues/4976) Add support for the `Filecoin.EthSubscribe` and `Filecoin.EthUnsubscribe` API methods to enable subscriptions to Ethereum event types: `heads` and `logs`.

- [#5999](https://github.com/ChainSafe/forest/pull/5999) Add `forest-cli mpool nonce` command to get the current nonce for an address.

### Changed

- [#5886](https://github.com/ChainSafe/forest/issues/5886) Updated `forest-tool archive metadata` to print F3 snapshot header info when applicable.

- [#5869](https://github.com/ChainSafe/forest/pull/5869) Updated `forest-cli snapshot export` to print average speed.

- [#5969](https://github.com/ChainSafe/forest/pull/5969) Updated `forest-tool snapshot validate` to print better error message for troubleshooting.

### Fixed

- [#5863](https://github.com/ChainSafe/forest/pull/5863) Fixed needless GC runs on a stateless node.

- [#5859](https://github.com/ChainSafe/forest/pull/5859) Fixed size calculation for zstd frame cache.

- [#5975](https://github.com/ChainSafe/forest/issues/5975) Fixed JSON escaping in the `Filecoin.ChainNotify` RPC method.

## Forest v0.28.0 "Denethor's Folly"

This is a non-mandatory release recommended for all node operators. It includes numerous fixes and quality-of-life improvements for development and archival snapshot operations. It also includes a memory leak fix that would surface on long-running nodes.

### Added

- [#5739](https://github.com/ChainSafe/forest/issues/5739) Add `--export-mode` flag to the `forest-tool archive sync-bucket` subcommand. This allows exporting and uploading only the required files.

- [#5778](https://github.com/ChainSafe/forest/pull/5778) Feat generate a detailed test report in `api compare` command through `--report-dir` and `--report-mode`.

### Changed

- [#5771](https://github.com/ChainSafe/forest/issues/5771) Update OpenRPC schemars by bumping `schemars` create.

- [#5816](https://github.com/ChainSafe/forest/pull/5816) Changed the monitoring stack to include a full Forest node. This allows for one-click local deployment of a fully-monitored Forest setup via `docker compose up` in `./monitored-stack`.

### Removed

- [#5822](https://github.com/ChainSafe/forest/issues/5822) Remove `mimalloc` feature.

### Fixed

- [#5752](https://github.com/ChainSafe/forest/issues/5752) Fix duplicated events in `Filecoin.ChainGetEvents` RPC method.

- [#5762](https://github.com/ChainSafe/forest/issues/5762) Cleanup temporary CAR DB files on node start.

- [#5773](https://github.com/ChainSafe/forest/pull/5773) The `forest-tool index backfill` now correctly respects the `--from` argument. At the same time, it's been made optional and will default to the chain head.

- [#5610](https://github.com/ChainSafe/forest/issues/5610) Fix `Filecoin.StateGetNetworkParams` and `Filecoin.StateNetworkName` RPC methods output for mainnet. They now return `mainnet` (and not `testnetnet`) as the network name, which is consistent with Lotus.

- [#5750](https://github.com/ChainSafe/forest/pull/5750) Fix regression causing the `Filecoin.ChainNotify` RPC endpoint to be unreachable.

- [#5730](https://github.com/ChainSafe/forest/issues/5730) Fixed various bugs in the mempool selection logic, including gas overpricing and incorrect message chain pruning. Additional logic was added to limit the number of messages in the block.

- [#5842](https://github.com/ChainSafe/forest/pull/5842) Fixed a memory leak in the bad block cache that could lead to excessive memory usage over time.

## Forest v0.27.0 "Whisperer in Darkness"

This is a non-mandatory but highly recommended release for all node operators. It introduces a fix for the forest node
getting stuck, a few new features including automatic snapshot GC scheduler, offline index backfilling, and
important bug fixes. It also contains a breaking change regarding the `detach` mode.

### Breaking

- [#5652](https://github.com/ChainSafe/forest/pull/5652) Remove `--detach` flag from `forest`. Checkout the [migration guide](https://github.com/ChainSafe/forest#detaching-forest-process)

### Added

- [#5598](https://github.com/ChainSafe/forest/pull/5598) Add `forest-cli chain prune snap` command for garbage collecting the database with a new snapshot garbage collector.

- [#5629](https://github.com/ChainSafe/forest/pull/5629) Save the default RPC token and consume it automatically.

- [#5639](https://github.com/ChainSafe/forest/pull/5639) Add automatic scheduler for snapshot GC.

- [#5697](https://github.com/ChainSafe/forest/pull/5697) Add `forest-cli chain list` command for viewing a segment of the chain.

- [#5666](https://github.com/ChainSafe/forest/pull/5666) Add support for the `Filecoin.StateReadState` API method to read the state of the actors.

- [#5738](https://github.com/ChainSafe/forest/pull/5738) Add support for the `forest-cli state read-state` command.

### Changed

- [#5616](https://github.com/ChainSafe/forest/pull/5616) Remove the initial background task for populating Ethereum mappings. Use `forest-tool index backfill` to perform this operation offline instead.

- [#5662](https://github.com/ChainSafe/forest/pull/5662) Print index size when applicable in `forest-tool archive info`.

### Fixed

- [#5624](https://github.com/ChainSafe/forest/pull/5624) Fix `Filecoin.EthTraceFilter` to correctly handle null tipsets and stay within the filter range.

- [#5177](https://github.com/ChainSafe/forest/issues/5177) Fix `Filecoin.EthGetBlockReceiptsLimited` to correctly handle the limit parameter.

- [#5704](https://github.com/ChainSafe/forest/pull/5704) Fixed an issue that a Forest node gets stuck in some cases.

- [#4490](https://github.com/ChainSafe/forest/issues/4490) Fixed panic conditions in the `Filecoin.MpoolSelect` RPC method.

## Forest v0.26.1 "Ijon Tichy"

This is a non-mandatory release for all node operators. It includes a fix for the F3 on mainnet and a few other improvements. It also sets the initial power table CID for F3 on mainnet.

### Breaking

- [#5559](https://github.com/ChainSafe/forest/pull/5559) Change `Filecoin.ChainGetMinBaseFee` to `Forest.ChainGetMinBaseFee` with read access.
- [#5589](https://github.com/ChainSafe/forest/pull/5589) Replace existing `Filecoin.SyncState` API with new `Forest.SyncStatus` to track node syncing progress specific to Forest.

### Added

- [#4750](https://github.com/ChainSafe/forest/issues/4750) Add support for `Filecoin.ChainGetEvents` RPC method. Add `index backfill` subcommand to `forest-tool`.
- [#5483](https://github.com/ChainSafe/forest/pull/5483) Add `forest-tool archive sync-bucket` command.

### Changed

- [#5467](https://github.com/ChainSafe/forest/pull/5467) Improve error message for `Filecoin.EthEstimateGas` and `Filecoin.EthCall`.

### Fixed

- [#5609](https://github.com/ChainSafe/forest/pull/5609) Fixed an issue with F3 on mainnet where the node would not join the KAD network.

## Forest v0.25.3 "Sméagol"

This is a non-mandatory, but highly recommended, release targeting the mainnet node operators. It includes a fix preventing the node from duplicate, costly migrations. Given the upcoming network upgrade state migration is expected to be slow, we recommend upgrading your Forest node before `Mon 14 Apr 23:00:00 UTC 2025`.

### Fixed

- [#5512](https://github.com/ChainSafe/forest/pull/5512) Fixed `Filecoin.EthTraceFilter` RPC method.
- [#5517](https://github.com/ChainSafe/forest/issues/5517) Fix the `forest-cli sync wait` issue
- [#5540](https://github.com/ChainSafe/forest/pull/5540) Avoid duplicate migrations.

## Forest v0.25.2 "Fool of a Took"

This is a mandatory release for mainnet and calibnet node operators. It introduces a fix upgrade for calibnet at epoch `2_558_014` which corresponds to `Mon  7 Apr 23:00:00 UTC 2025` and changes the mainnet upgrade epoch for the NV25 _Teep_ to `4_878_840` which corresponds to `Mon 14 Apr 23:00:00 UTC 2025`.

See [here](https://github.com/filecoin-project/community/discussions/74#discussioncomment-12720764) for details on the issue.

### Changed

- [#5515](https://github.com/ChainSafe/forest/pull/5515) Changed the mainnet upgrade epoch for the NV25 _Teep_ to `4_878_840` which corresponds to `Mon 14 Apr 23:00:00 UTC 2025`.

### Fixed

- [#5515](https://github.com/ChainSafe/forest/pull/5515) Introduced a fix network upgrade for the calibnet `Tock`.

## Forest v0.25.1 "Goldberry"

This is a mandatory release for mainnet node operators. It includes the NV25 _Teep_ network upgrade at epoch `4_867_320` which corresponds to `10 Apr 23:00:00 UTC 2025`. This release also includes a few fixes, most notably a database migration speed up that used to give certain important people a massive headache. Forest Prometheus metrics have been cleaned and you can look them up in the [documentation](https://docs.forest.chainsafe.io/reference/metrics).

### Breaking

- [#5464](https://github.com/ChainSafe/forest/pull/5464) Changed the `allow_response_mismatch` flag to `use_response_from` in the `forest-tool api generate-test-snapshot` subcommand. This allows specifying the source of the response to use when generating test snapshots and creating failing tests.

### Added

- [#5461](https://github.com/ChainSafe/forest/pull/5461) Add `forest-tool shed migrate-state` command.
- [#5488](https://github.com/ChainSafe/forest/pull/5488) Add partial support for the `Filecoin.StateCompute` RPC method.

### Changed

- [#5452](https://github.com/ChainSafe/forest/pull/5452) Speed up void database migration.
- [#5479](https://github.com/ChainSafe/forest/pull/5479) Move the snapshot progress tracking to sync wait command

### Removed

- [#5449](https://github.com/ChainSafe/forest/pull/5449) Remove unnecessary/duplicate metrics.
- [#5457](https://github.com/ChainSafe/forest/pull/5457) Remove most prometheus
  metrics starting with `libp2p_`.

### Fixed

- [#5458](https://github.com/ChainSafe/forest/pull/5458) Fix stack overflow occurring when running Forest in debug mode.
- [#5475](https://github.com/ChainSafe/forest/pull/5475) Added missing fields for state sector RPC methods that were added in the recent network upgrade.

## Forest v0.25.0 "Bombadil"

This is a mandatory release for calibnet node operators. It includes the revised NV25 _Teep_ network upgrade at epoch `2_523_454` which corresponds to `2025-03-26T23:00:00Z`. This release also includes a number of new RPC methods, fixes and other improvements. Be sure to check the breaking changes before upgrading.

### Breaking

- [#4505](https://github.com/ChainSafe/forest/issues/4505) The Ethereum RPC API indexer now runs as a background task (disabled by default). It can be configured in the `[chain_indexer]` section or via the `FOREST_CHAIN_INDEXER_ENABLED` environment variable. The `client.eth_mapping_ttl` option has been moved to `chain_indexer.gc_retention_epochs`, which is now specified as a number of epochs.

### Added

- [#5375](https://github.com/ChainSafe/forest/issues/5375) Add an RNG wrapper that that can be overriden by a reproducible seeded RNG.

- [#5386](https://github.com/ChainSafe/forest/pull/5386) Add support for the `Filecoin.EthTraceTransaction` RPC method.

- [#5427](https://github.com/ChainSafe/forest/pull/5427) Add support for the `Filecoin.EthTraceFilter` RPC method.

- [#5383](https://github.com/ChainSafe/forest/pull/5383) Add support for `Filecoin.EthGetFilterChanges` RPC method.

### Fixed

- [#5377](https://github.com/ChainSafe/forest/pull/5377) Fix incorrect handling of `max_height` for `latest` predefined block in `Filecoin.EthGetLogs`.

- [#5356](https://github.com/ChainSafe/forest/issues/5356) Fixed slow (and incorrect!) `Filecoin.EthGasPrice` RPC method. The method now returns the correct gas price of the latest tipset.

## Forest v0.24.0 "Treebeard"

Non-mandatory release without network upgrades. It includes a number of potentially breaking changes (see below), new RPC methods, fixes and other improvements.

### Breaking

- [#5236](https://github.com/ChainSafe/forest/pull/5236) Dropped support for migrating from ancient versions of Forest. The latest supported version for migration is [v0.19.2](https://github.com/ChainSafe/forest/releases/tag/v0.19.2).

- [#4261](https://github.com/ChainSafe/forest/issues/4261) Remove the short flags from `forest-wallet list` and `forest-wallet balance` commands.

- [#5329](https://github.com/ChainSafe/forest/pull/5329) Changed JSON-RPC alias for `Filecoin.NetListening`, `Filecoin.NetVersion`, `Filecoin.EthTraceBlock`, `Filecoin.EthTraceReplayBlockTransactions` to adhere to Lotus API.

### Added

- [#5244](https://github.com/ChainSafe/forest/issues/5244) Add `live` and `healthy` subcommands to `forest-cli healthcheck`.

- [#4708](https://github.com/ChainSafe/forest/issues/4708) Add support for the
  `Filecoin.EthTraceBlock` RPC method.

- [#5154](https://github.com/ChainSafe/forest/pull/5154) Added support for test criteria overrides in `forest-tool api compare`.

- [#5167](https://github.com/ChainSafe/forest/pull/5167) Allow overriding drand configs with environment variables.

- [#4851](https://github.com/ChainSafe/forest/issues/4851) Add support for `FOREST_MAX_FILTER_RESULTS` in `Filecoin.EthGetLogs` RPC method.
  Add an `[events]` section to Forest configuration file.

- [#4954](https://github.com/ChainSafe/forest/issues/4954) Add `--format json` to `forest-cli chain head` command.

- [#5232](https://github.com/ChainSafe/forest/issues/5232) Support `CARv2` stream decoding.

- [#5230](https://github.com/ChainSafe/forest/issues/5230) Add `CARv2` support to `forest-tool archive` command.

- [#5259](https://github.com/ChainSafe/forest/issues/5259) Add `forest-cli wait-api` command.

- [#4769](https://github.com/ChainSafe/forest/issues/4769) Add delegated address support to `forest-wallet new` command.

- [#5147](https://github.com/ChainSafe/forest/issues/5147) Add support for the `--rpc-filter-list` flag to the `forest` daemon. This flag allows users to specify a list of RPC methods to whitelist or blacklist.

- [#4709](https://github.com/ChainSafe/forest/issues/4709) Add support for `Filecoin.EthTraceReplayBlockTransactions` RPC method.

- [#4751](https://github.com/ChainSafe/forest/issues/4751) Add support for `Filecoin.GetActorEventsRaw` RPC method.

- [#4671](https://github.com/ChainSafe/forest/issues/4671) Add support for `Filecoin.EthGetFilterLogs` RPC method.

- [#5309](https://github.com/ChainSafe/forest/issues/5309) Add `forest-tool shed f3 check-activation-raw` command.

- [#5242](https://github.com/ChainSafe/forest/issues/5242) Fix existing signature verification and add `delegated` signature

- [#5314](https://github.com/ChainSafe/forest/pull/5314) Add omit fields option for OpenRPC spec generation.

- [#5293](https://github.com/ChainSafe/forest/issues/5293) Extend RPC test snapshots to support Eth methods that require an index.

- [#5319](https://github.com/ChainSafe/forest/pull/5319) Improve the ergonomics of the `forest-tool api generate-test-snapshot` subcommand.

- [#5342](https://github.com/ChainSafe/forest/pull/5342) Improved the ergonomics of handling addresses in the `Filecoin.EthGetLogs` and `Filecoin.EthNewFilter` to accept both single addresses and arrays of addresses. This conforms to the Ethereum RPC API.

- [#5346](https://github.com/ChainSafe/forest/pull/5346) `Filecoin.EthGetBlockReceipts` and `Filecoin.EthGetBlockReceiptsLimited` now accepts predefined block parameters on top of the block hash, e.g., `latest`, `earliest`, `pending`.

- [#5324](https://github.com/ChainSafe/forest/pull/5324) Add shell completion subcommand in `forest-tool`

- [#5368](https://github.com/ChainSafe/forest/pull/5368) Add `Forest.SyncSnapshotProgress` RPC and track the progress in `forest-cli sync status`

### Changed

- [#5237](https://github.com/ChainSafe/forest/pull/5237) Stylistic changes to FIL pretty printing.

- [#5329](https://github.com/ChainSafe/forest/pull/5329) `Filecoin.Web3ClientVersion` now returns the name of the node and its version, e.g., `forest/0.23.3+git.32a34e92`.

- [#5332](https://github.com/ChainSafe/forest/pull/5332) Adhere to the Ethereum RPC API for `eth_call` by not requiring the `from`, `gas`, `gas_price` and `value` parameters. `data` is still required.

- [#5359](https://github.com/ChainSafe/forest/pull/5359) Eth RPC API methods' params are now all in _camelCase_. This aligns with the Ethereum RPC API. Note that this change is only for the OpenRPC documentation and does not affect the actual RPC methods which accepted correct _camelCase_ params before.

### Removed

- [#5344](https://github.com/ChainSafe/forest/pull/5344) Removed the last traces of the `forest-cli attach` command.

### Fixed

- [#5111](https://github.com/ChainSafe/forest/issues/5111) Make F3 work when the node Kademlia is disabled.

- [#5122](https://github.com/ChainSafe/forest/issues/5122) Fix a bug in database garbage collection flow.

- [#5131](https://github.com/ChainSafe/forest/pull/5131) Fix incorrect data deserialization in the `Filecoin.EthGetBlockReceipts` RPC method. This caused the method to return an error on some blocks.

- [#5150](https://github.com/ChainSafe/forest/pull/5150) Fix incorrect prototype for the `Filecoin.EthGetBlockReceiptsLimited` RPC method.

- [#5006](https://github.com/ChainSafe/forest/issues/5006) Fix incorrect logs, logs bloom and event index for the `Filecoin.EthGetBlockReceipts` RPC method.

- [#4996](https://github.com/ChainSafe/forest/issues/4996) Fix incorrect logs and logs bloom for the `Filecoin.EthGetTransactionReceipt` and
  `Filecoin.EthGetTransactionReceiptLimited` RPC methods on some blocks.

- [#5213](https://github.com/ChainSafe/forest/issues/5213) Fix incorrect results for the `Filecoin.EthGetLogs` RPC method on ranges that include null tipsets.

- [#5357](https://github.com/ChainSafe/forest/issues/5357) Make data field in EthCallMessage optional. Affected RPC methods are `Filecoin.EthEstimateGas`(`eth_estimateGas`) and `Filecoin.EthCall`(`eth_call`)

- [#5345](https://github.com/ChainSafe/forest/pull/5345) Fixed handling of odd-length hex strings in some Eth RPC methods. Now, the methods should not return error if provided with, e.g., `0x0` (which would be expanded to `0x00`).

## Forest v.0.23.3 "Plumber"

Mandatory release for calibnet node operators. It fixes a sync error at epoch 2281645.

### Breaking

### Added

- [#5020](https://github.com/ChainSafe/forest/issues/5020) Add support for the
  `Filecoin.EthGetTransactionByBlockNumberAndIndex` RPC method.

- [#4907](https://github.com/ChainSafe/forest/issues/4907) Add support for the
  `Filecoin.StateMinerInitialPledgeForSector` RPC method.

### Changed

### Removed

- [#5077](https://github.com/ChainSafe/forest/pull/5077) Remove
  `peer_tipset_epoch` from the metrics.

### Fixed

- [#5109](https://github.com/ChainSafe/forest/pull/5109) Fix a calibnet sync error at epoch 2281645.

## Forest v.0.23.2 "Feint"

Mandatory release for calibnet node operators. It removes the NV25 _Teep_ network upgrade from the schedule. Read more [here](https://github.com/filecoin-project/community/discussions/74#discussioncomment-11549619).

### Changed

- [#5079](https://github.com/ChainSafe/forest/pull/5079) Removed the NV25 _Teep_ network upgrade for calibnet from the schedule. It is postponed to a later date.

## Forest v0.23.1 "Lappe"

### Fixed

- [#5071](https://github.com/ChainSafe/forest/pull/5071) Fix issue that caused
  Forest to temporarily drift out of sync.

## Forest v0.23.0 "Saenchai"

This is a mandatory release for the calibration network. It includes the NV25
_Teep_ network upgrade at epoch `2_235_454` which corresponds to
`Mon 16 Dec 23:00:00 UTC 2024`. This release also includes a number of new RPC
methods, fixes (notably to the garbage collection), and other improvements.

### Added

- [#5010](https://github.com/ChainSafe/forest/pull/5010) Added
  `forest-cli f3 certs list` CLI command.

- [#4995](https://github.com/ChainSafe/forest/pull/4995) Added
  `forest-cli f3 powertable get` CLI command.

- [#5028](https://github.com/ChainSafe/forest/pull/5028) Added
  `forest-cli f3 powertable get-proportion` CLI command.

- [#5054](https://github.com/ChainSafe/forest/pull/5054) Added `--dump-dir`
  option to `forest-tool api compare` CLI command.

- [#4704](https://github.com/ChainSafe/forest/issues/4704) Add support for the
  `Filecoin.EthGetTransactionReceiptLimited` RPC method.

- [#4875](https://github.com/ChainSafe/forest/issues/4875) Move
  fil-actor-interface crate from fil-actor-states repo.

- [#4701](https://github.com/ChainSafe/forest/issues/4701) Add support for the
  `Filecoin.EthGetTransactionByBlockHashAndIndex` RPC method.

### Changed

- [#5053](https://github.com/ChainSafe/forest/pull/5053) Added support for the
  NV25 _Teep_ network upgrade for `2k` and `butterflynet` networks.

- [#5040](https://github.com/ChainSafe/forest/issues/5040) Added support for the
  NV25 _Teep_ network upgrade for `calibration` network.

### Fixed

- [#4959](https://github.com/ChainSafe/forest/pull/4959) Re-enable garbage
  collection after implementing a "persistent" storage for manifests.

- [#4988](https://github.com/ChainSafe/forest/pull/4988) Fix the `logs` member
  in `EthTxReceipt` that was initialized with a default value.

- [#5043](https://github.com/ChainSafe/forest/pull/5043) Added missing entry for
  `TukTuk` upgrade in the `Filecoin.StateGetNetworkParams` RPC method.

## Forest 0.22.0 "Pad Thai"

Mandatory release for mainnet node operators. It sets the upgrade epoch for the
NV24 _Tuk Tuk_ upgrade to `4_461_240` which corresponds to
`Wed 20 Nov 23:00:00 UTC 2024`. No F3 support is planned for the NV24, see
[this post](https://github.com/filecoin-project/core-devs/discussions/150#discussioncomment-11164504)
for more details.

### Breaking

- [#4952](https://github.com/ChainSafe/forest/pull/4952) Extended the
  `forest-cli chain head` command to allow for specifying number of last tipsets
  to display. This change is breaking as the output now contains the epoch of
  tipsets.

### Added

- [#4937](https://github.com/ChainSafe/forest/pull/4937) Added
  `forest-cli f3 manifest` CLI command.

- [#4949](https://github.com/ChainSafe/forest/pull/4949) Added
  `forest-cli f3 status` CLI command.

- [#4949](https://github.com/ChainSafe/forest/pull/4949) Added
  `forest-cli f3 certs get` CLI command.

- [#4706](https://github.com/ChainSafe/forest/issues/4706) Add support for the
  `Filecoin.EthSendRawTransaction` RPC method.

- [#4839](https://github.com/ChainSafe/forest/issues/4839) Add support for the
  `Filecoin.EthGetBlockReceipts` RPC method.

- [#5017](https://github.com/ChainSafe/forest/issues/5017) Add support for the
  `Filecoin.EthGetBlockReceiptsLimited` RPC method.

- [#4943](https://github.com/ChainSafe/forest/pull/4943) Add generation of
  method aliases for `forest-tool shed openrpc` subcommand and sort all methods
  in lexicographic order.

- [#4801](https://github.com/ChainSafe/forest/issues/4801) Add support for
  `Tuk Tuk` NV24 upgrade for mainnet

## Forest 0.21.1 "Songthaew Plus"

This is an optional release for calibration network node operators. It enables
F3 by default and includes initial power table CID on calibration network.

### Breaking

### Added

- [#4910](https://github.com/ChainSafe/forest/issues/4910) Add support for the
  `Filecoin.F3ListParticipants` RPC method.

- [#4920](https://github.com/ChainSafe/forest/issues/4920) Add support for the
  `Filecoin.F3GetOrRenewParticipationTicket` RPC method.

- [#4924](https://github.com/ChainSafe/forest/issues/4924) Add support for the
  `Filecoin.F3GetManifest` RPC method.

- [#4917](https://github.com/ChainSafe/forest/issues/4917) Support `dnsaddr` in
  the bootstrap list.

- [#4939](https://github.com/ChainSafe/forest/issues/4939) Fix
  `Filecoin.EthBlockNumber` RPC method return type to be an `EthUInt64`.

### Changed

- [#4920](https://github.com/ChainSafe/forest/issues/4920) Update
  `Filecoin.F3Participate` RPC method to align with the spec change.

- [#4920](https://github.com/ChainSafe/forest/issues/4920) Update
  `Filecoin.F3ListParticipants` RPC method to align with the spec change.

### Removed

- [#4927](https://github.com/ChainSafe/forest/pull/4927) Temporarily disable
  garbage collection.

### Fixed

## Forest 0.21.0 "Songthaew"

This is a mandatory release for calibration network node operators. It includes
state migration for the NV24 _TukTuk_ upgrade at epoch `2078794`
2024-10-23T13:30:00Z. It also includes a number of new RPC methods, fixes and F3
support.

### Breaking

- [#4782](https://github.com/ChainSafe/forest/pull/4782) Devnets are no longer
  configurable with legacy drand network.

### Added

- [#4703](https://github.com/ChainSafe/forest/issues/4703) Add support for the
  `Filecoin.EthGetTransactionByHashLimited` RPC method.

- [#4783](https://github.com/ChainSafe/forest/issues/4783) Add support for the
  `Filecoin.NetProtectList` RPC method.

- [#4865](https://github.com/ChainSafe/forest/issues/4865) Add support for the
  `Filecoin.F3IsRunning` RPC method.

- [#4878](https://github.com/ChainSafe/forest/issues/4878) Add support for the
  `Filecoin.F3GetProgress` RPC method.

- [#4857](https://github.com/ChainSafe/forest/pull/4857) Add support for nv24
  (TukTuk).

### Changed

- [#4786](https://github.com/ChainSafe/forest/issues/4786) ubuntu image is
  upgraded from 22.04 to 24.04 in Dockerfile

### Fixed

- [#4809](https://github.com/ChainSafe/forest/issues/4777) the Mac OS X build on
  Apple silicons works
- [#4820](https://github.com/ChainSafe/forest/pull/4820) Fix edge-case in
  `Filecoin.MinerGetBaseInfo` RPC method.

- [#4890](https://github.com/ChainSafe/forest/issues/4890) Fix incorrect deal
  weight calculation in the `Filecoin.StateMinerInitialPledgeCollateral` RPC
  method.

## Forest 0.20.0 "Brexit"

Non-mandatory release including a number of new RPC methods, fixes, and other
improvements. Be sure to check the breaking changes before upgrading.

### Breaking

- [#4620](https://github.com/ChainSafe/forest/pull/4620) Removed the
  `--consume-snapshot` parameter from the `forest` binary. To consume a
  snapshot, use `--import-snapshot <path> --import-mode=move`.

- [#3403](https://github.com/ChainSafe/forest/issues/3403) The snapshot
  validation command `forest-tool snapshot validate` now checks the snapshots
  individually. The previous behavior, to validate the sum of the snapshots, can
  be achieved via `forest-tool snapshot validate-diffs`.

- [#4672](https://github.com/ChainSafe/forest/issues/4672) The default user in
  Docker images is now `root`. This facilitates usage, especially when mounting
  volumes and dealing with surprising permission errors. Note that the default
  data directory is now `/root/.local/share/forest` and not
  `/home/forest/.local/share/forest`. The directory will **not** be migrated
  automatically. Please adapt your configurations accordingly. If you've been
  switching to `root` manually in your workflows you can now remove that step.

- [#4757](https://github.com/ChainSafe/forest/pull/4757) Changed the default
  option of `--import-mode` to `auto` which hardlink snapshots and fallback to
  copying them if not applicable.

- [#4768](https://github.com/ChainSafe/forest/pull/4768) Moved all RPC methods
  to V1 when applicabile

### Added

- [#3959](https://github.com/ChainSafe/forest/issues/3959) Added support for the
  Ethereum RPC name aliases.

- [#4607](https://github.com/ChainSafe/forest/pull/4607) Expose usage and timing
  metrics for RPC methods.

- [#4599](https://github.com/ChainSafe/forest/issues/4599) Block delay and block
  propagation delays are now configurable via
  [environment variables](https://github.com/ChainSafe/forest/blob/main/documentation/src/environment_variables.md).

- [#4596](https://github.com/ChainSafe/forest/issues/4596) Support
  finality-related params in the `Filecoin.EthGetBlockByNumber` RPC method.

- [#4620](https://github.com/ChainSafe/forest/pull/4620) Added an option to link
  snapshots instead of moving or copying them. This can be invoked with
  `--import-snapshot <path> --import-mode=symlink`.

- [#4533](https://github.com/ChainSafe/forest/pull/4641) Added `build_info`
  metric to Prometheus metrics, which include the current build's version.

- [#4628](https://github.com/ChainSafe/forest/issues/4628) Added support for
  devnets (2k networks) in the offline Forest.

- [#4463](https://github.com/ChainSafe/forest/issues/4463) Add support for the
  `Filecoin.EthGetTransactionByHash` RPC method.

- [#4613](https://github.com/ChainSafe/forest/issues/4613) Add support for the
  `Filecoin.EthCall` RPC method.

- [#4665](https://github.com/ChainSafe/forest/issues/4665) Add support for the
  `Filecoin.EthNewFilter` RPC method.

- [#4666](https://github.com/ChainSafe/forest/issues/4666) Add support for the
  `Filecoin.EthNewBlockFilter` RPC method.

- [#4667](https://github.com/ChainSafe/forest/issues/4667) Add support for the
  `Filecoin.EthNewPendingTransactionFilter` RPC method.

- [#4686](https://github.com/ChainSafe/forest/issues/4686) Add support for the
  `Filecoin.EthAddressToFilecoinAddress` RPC method.

- [#4612](https://github.com/ChainSafe/forest/issues/4612) Add support for the
  `Filecoin.MarketAddBalance` RPC method.

- [#4701](https://github.com/ChainSafe/forest/issues/4701) Add method
  `Filecoin.EthGetTransactionByBlockHashAndIndex` to existing methods (though
  without support, which matches the current Lotus's behavior).

- [#4702](https://github.com/ChainSafe/forest/issues/4702) Add method
  `Filecoin.EthGetTransactionByBlockNumberAndIndex` to existing methods (though
  without support, which matches the current Lotus's behavior).

- [#4757](https://github.com/ChainSafe/forest/pull/4757) Added an option to
  hardlink snapshots instead of moving or copying them. This can be invoked with
  `--import-snapshot <path> --import-mode=hardlink`.

- [#4668](https://github.com/ChainSafe/forest/issues/4668) Add support for the
  `Filecoin.EthUninstallFilter` RPC method.

### Changed

- [#4583](https://github.com/ChainSafe/forest/pull/4583) Removed the expiration
  date for the master token. The new behavior aligns with Lotus.

### Removed

- [#4624](https://github.com/ChainSafe/forest/pull/4624) Remove the
  `--chain-import` flag. Its functionality can be accessed through the more
  flexible `--height` flag.

### Fixed

- [#4603](https://github.com/ChainSafe/forest/pull/4603) Fixed incorrect
  deserialisation in `Filecoin.EthGetBlockByNumber` and
  `Filecoin.EthGetBlockByHash` RPC methods.

- [#4610](https://github.com/ChainSafe/forest/issues/4610) Fixed incorrect
  structure in the `Filecoin.MinerGetBaseInfo` RPC method.

- [#4635](https://github.com/ChainSafe/forest/pull/4635) Fixed bug in
  `StateMinerProvingDeadline`.

- [#4674](https://github.com/ChainSafe/forest/pull/4674) Fixed bug in
  `StateCirculatingSupply`.

- [#4656](https://github.com/ChainSafe/forest/pull/4656) Fixed bug in
  `StateCall`.

- [#4498](https://github.com/ChainSafe/forest/issues/4498) Fixed incorrect
  `Filecoin.Version`s `APIVersion` field value.

## Forest 0.19.2 "Eagle"

Non-mandatory release that includes a fix for the Prometheus-incompatible
metric.

### Fixed

- [#4594](https://github.com/ChainSafe/forest/pull/4594) Reverted the Forest
  version metric with Prometheus-incompatible metric type.

## Forest 0.19.1 "Pathfinder"

Mandatory release for mainnet nodes that adds the NV23 _Waffle_ migration at
epoch 4154640 (2024-08-06T12:00:00Z). This release also adds support for new RPC
methods and fixes a networking issue where Forest would not bootstrap a Lotus
node.

### Added

- [#4545](https://github.com/ChainSafe/forest/pull/4545) Add support for the
  `Filecoin.StateGetAllClaims` RPC method.

- [#4545](https://github.com/ChainSafe/forest/pull/4545) Add support for the
  `Filecoin.StateGetAllAllocations` RPC method.

- [#4503](https://github.com/ChainSafe/forest/pull/4503) Add support for the
  `Filecoin.StateMinerAllocated` RPC method.

- [#4512](https://github.com/ChainSafe/forest/pull/4512) Add support for the
  `Filecoin.StateGetAllocationIdForPendingDeal` RPC method.

- [#4514](https://github.com/ChainSafe/forest/pull/4514) Add support for the
  `Filecoin.WalletSignMessage` RPC method.

- [#4517](https://github.com/ChainSafe/forest/pull/4517) Add support for the
  `Filecoin.StateGetAllocationForPendingDeal` RPC method.

- [#4526](https://github.com/ChainSafe/forest/pull/4526) Added
  `forest-cli state compute` method, and a corresponding RPC method
  `Forest.StateCompute`.

- [#4511](https://github.com/ChainSafe/forest/pull/4511) Add support for the
  `Filecoin.EthMaxPriorityFeePerGas` RPC method.

- [#4515](https://github.com/ChainSafe/forest/pull/4515) Add support for the
  `Filecoin.StateLookupRobustAddress` RPC method.

- [#4496](https://github.com/ChainSafe/forest/pull/4496) Add support for the
  `Filecoin.EthEstimateGas` RPC method.

- [#4558](https://github.com/ChainSafe/forest/pull/4558) Add support for the
  `Filecoin.StateVerifiedRegistryRootKey` RPC method.

- [#4474](https://github.com/ChainSafe/forest/pull/4474) Add new subcommand
  `forest-cli healthcheck ready`.

- [#4569](https://github.com/ChainSafe/forest/pull/4569) Add support for the
  `Filecoin.NetFindPeer` RPC method.

- [#4565](https://github.com/ChainSafe/forest/pull/4565) Add support for the
  `Filecoin.StateGetRandomnessDigestFromBeacon` RPC method.

- [#4547](https://github.com/ChainSafe/forest/pull/4547) Add support for the
  `Filecoin.MpoolPushUntrusted` RPC method.

- [#4561](https://github.com/ChainSafe/forest/pull/4561) Add support for the
  `Filecoin.MpoolBatchPush` and `Filecoin.MpoolBatchPushUntrusted` RPC method.

- [#4566](https://github.com/ChainSafe/forest/pull/4566) Add support for the
  `Filecoin.StateGetRandomnessDigestFromTickets` RPC method.

## Forest 0.19.0 "Pastel de nata"

This is a mandatory release for all calibration network node operators. It
includes migration logic for the NV23 _Waffle_ network upgrade. It also includes
a number of new RPC methods, fixes to existing ones, and other improvements,
most notably, garbage collection fix.

### Added

- [#4473](https://github.com/ChainSafe/forest/pull/4473) Add support for NV23
  _Waffle_ network upgrade (FIP-0085, FIP-0091, v14 actors).

- [#4352](https://github.com/ChainSafe/forest/pull/4352) Add support for the
  `Filecoin.StateGetClaim` RPC method.

- [#4356](https://github.com/ChainSafe/forest/pull/4356) Add support for the
  `Filecoin.NetProtectAdd` RPC method.

- [#4382](https://github.com/ChainSafe/forest/pull/4382) Add support for the
  `Filecoin.StateGetAllocation` RPC method.

- [#4381](https://github.com/ChainSafe/forest/pull/4381) Add support for the
  `Filecoin.StateSectorPartition` RPC method.

- [#4368](https://github.com/ChainSafe/forest/issues/4368) Add support for the
  `Filecoin.EthGetMessageCidByTransactionHash` RPC method.

- [#4167](https://github.com/ChainSafe/forest/issues/4167) Add support for the
  `Filecoin.EthGetBlockByHash` RPC method.

- [#4360](https://github.com/ChainSafe/forest/issues/4360) Add support for the
  `Filecoin.EthGetBlockTransactionCountByHash` RPC method.

- [#4475](https://github.com/ChainSafe/forest/pull/4475) Add support for the
  `Filecoin.EthFeeHistory` RPC method.

- [#4359](https://github.com/ChainSafe/forest/issues/4359) Add support for the
  `EIP-1898` object scheme.

- [#4443](https://github.com/ChainSafe/forest/issues/4443) Update
  `Filecoin.StateSectorPreCommitInfo` RPC method to be API-V1-compatible

- [#4444](https://github.com/ChainSafe/forest/issues/4444) Update
  `Filecoin.StateWaitMsg` RPC method to be API-V1-compatible

### Removed

- [#4358](https://github.com/ChainSafe/forest/pull/4358) Remove the
  `forest-cli attach` command.

### Fixed

- [#4425](https://github.com/ChainSafe/forest/pull/4425) Fix GC collision
  issues.

- [#4357](https://github.com/ChainSafe/forest/pull/4357) Fix schema bug in the
  `Filecoin.ChainNotify` RPC method.

- [#4371](https://github.com/ChainSafe/forest/pull/4371) Fix extra `Apply`
  change in the `Filecoin.ChainNotify` RPC method.

- [#4002](https://github.com/ChainSafe/forest/issues/4002) Add support for
  multiple WebSocket clients for `Filecoin.ChainNotify` RPC method.

- [#4390](https://github.com/ChainSafe/forest/issues/4390) Fix `SignedMessage`
  JSON formatting to match Lotus.

## Forest 0.18.0 "Big Bang"

This is a non-mandatory release including a fair number of new RPC methods and
improvements to the Forest RPC API. The release also includes a number of bug
fixes, as outlined below. Please note the breaking changes in this release.

### Breaking

- [#4177](https://github.com/ChainSafe/forest/pull/4177) Rename environment
  variable `TRUST_PARAMS` to `FOREST_FORCE_TRUST_PARAMS`.

- [#4184](https://github.com/ChainSafe/forest/pull/4184) Removed short form
  flags from `forest` binary.

- [#4215](https://github.com/ChainSafe/forest/pull/4215) Changed the prefix for
  Forest-specific RPC methods to `Forest`; `Filecoin.NetInfo` and
  `Filecoin.StateFetchRoot` to `Forest.NetInfo` and `Forest.StateFetchRoot`.

- [#4262](https://github.com/ChainSafe/forest/pull/4262) Added `Bearer` prefix
  to the `Authorization` header in the Forest RPC API. This is a
  partially-breaking change - new Forest RPC clients will not work with old
  Forest nodes. This change is necessary to align with the Lotus RPC API.

### Added

- [#4246](https://github.com/ChainSafe/forest/pull/4246) Add support for the
  `Filecoin.SyncSubmitBlock` RPC method.

- [#4084](https://github.com/ChainSafe/forest/pull/4084) Add support for the
  `Filecoin.StateDealProviderCollateralBounds` RPC method.

- [#3949](https://github.com/ChainSafe/forest/issues/3949) Added healthcheck
  endpoints `/healthz`, `/readyz`, and `/livez`. By default, the healthcheck
  endpoint is enabled on port 2346.

- [#4166](https://github.com/ChainSafe/forest/issues/4166) Add support for the
  `Filecoin.Web3ClientVersion` RPC method.

- [#4184](https://github.com/ChainSafe/forest/pull/4184) Added
  `--no-healthcheck` flag to `forest` to disable the healthcheck endpoint.

- [#4183](https://github.com/ChainSafe/forest/issues/4183) Add support for the
  `Filecoin.EthGetBlockByNumber` RPC method.

- [#4253](https://github.com/ChainSafe/forest/pull/4253) RPC client default
  timeout is now configurable via the `FOREST_RPC_DEFAULT_TIMEOUT` environment
  variable.

- [#4240](https://github.com/ChainSafe/forest/pull/4240) Added `--fixed-unit`
  and `--exact-balance` flags to `forest-wallet balance` similarly to
  `forest-wallet list` subcommand.

- [#4213](https://github.com/ChainSafe/forest/issues/4213) Add support for the
  `Filecoin.StateMinerInitialPledgeCollateral` RPC method.

- [#4214](https://github.com/ChainSafe/forest/issues/4214) Add support for the
  `Filecoin.StateMinerPreCommitDepositForPower` RPC method.

- [#4255](https://github.com/ChainSafe/forest/pull/4255) Add support for the
  `Filecoin.MinerCreateBlock` RPC method.

- [#4315](https://github.com/ChainSafe/forest/pull/4315) Add support for the
  `Filecoin.StateGetNetworkParams` RPC method.

- [#4326](https://github.com/ChainSafe/forest/pull/4326) Added
  `expected_network_height` metric to the Prometheus metrics.

### Changed

- [#4170](https://github.com/ChainSafe/forest/pull/4170) Change the default
  Filecoin proof parameters source to ChainSafe's hosted Cloudflare R2 bucket.
  IPFS gateway can still be enforced via `FOREST_PROOFS_ONLY_IPFS_GATEWAY=1`.

### Removed

### Fixed

- [#4177](https://github.com/ChainSafe/forest/pull/4177) Fixed a bug where the
  environment variable `IPFS_GATEWAY` was not used to change the IPFS gateway.

- [#4267](https://github.com/ChainSafe/forest/pull/4267) Fixed potential panics
  in `forest-tool api compare`.

- [#4297](https://github.com/ChainSafe/forest/pull/4297) Fixed double decoding
  of message in the `Filecoin.WalletSign` RPC method.

- [#4314](https://github.com/ChainSafe/forest/issues/4314) Fixed incorrect
  allowed proof types for all networks.

- [#4328](https://github.com/ChainSafe/forest/pull/4328) Fix issues when
  connecting to a network with fewer than 5 peers.

## Forest 0.17.2 "Dovakhin"

This is a **mandatory** release for all mainnet node operators. It changes the
NV22 _dragon_ network upgrade epoch to 3855360 (Wed Apr 24 02:00:00 PM UTC
2024)). All mainnet node **must** be updated to this version before the network
upgrade epoch to avoid being stuck on a fork.

### Changed

- [#4151](https://github.com/ChainSafe/forest/pull/4151) Changed the Dragon NV22
  network upgrade epoch to 3855360 (April 24th 2024).

### Fixed

- [#4145](https://github.com/ChainSafe/forest/pull/4145) Fix the
  `forest-cli net peers --agent` command in case the agent is not available.

## Forest 0.17.1 "Villentretenmerth"

This is a mandatory release that includes scheduled migration for the NV22
_Dragon_ network upgrade for mainnet and fix for the calibration network.
Various other fixes and improvements are included as well, see below for
details.

### Added

- [#4029](https://github.com/ChainSafe/forest/pull/4029) Add
  `forest-tool shed private-key-from-key-pair` and
  `forest-tool shed key-pair-from-private-key` commands. These facilate moving
  between Forest and Lotus without losing the peer-to-peer identity.

- [#4052](https://github.com/ChainSafe/forest/pull/4052) Add
  `forest-cli net reachability` command that prints information about
  reachability from the internet.

- [#4058](https://github.com/ChainSafe/forest/issues/4058) Add support for
  multiple snapshot files in the `forest-tool api serve` command.

- [#4056](https://github.com/ChainSafe/forest/pull/4056) Enable libp2p `quic`
  protocol

- [#4071](https://github.com/ChainSafe/forest/pull/4071) Add
  `forest-tool net ping` command that pings a peer via its multiaddress.

- [#4119](https://github.com/ChainSafe/forest/pull/4119) Add support for NV22
  fix for calibration network.

### Removed

- [#4018](https://github.com/ChainSafe/forest/pull/4018) Remove --ws flag from
  `forest-tool api compare`.

### Fixed

- [#4068](https://github.com/ChainSafe/forest/pull/4068) Fix schema bug in the
  `ChainNotify` RPC method.

- [#4080](https://github.com/ChainSafe/forest/pull/4080) Fix broken
  `StateVMCirculatingSupplyInternal` RPC method on calibnet.

- [#4091](https://github.com/ChainSafe/forest/pull/4091) Restore `Breeze`,
  `Smoke`, and `Ignition` entries for calibnet

- [#4093](https://github.com/ChainSafe/forest/pull/4093) Fix parsing issue in
  the `Filecoin.StateAccountKey` RPC method.

## Forest 0.17.0 "Smaug"

Mandatory release that includes:

- support for the NV22 _Dragon_ network upgrade, together with the required
  state migration,
- important networking improvements that increase Forest resilience to network
  disruptions,
- various improvements and support for new RPC methods.

### Added

- [#3555](https://github.com/ChainSafe/forest/issues/3555) Add Forest database
  query optimizations when serving with many car files.

- [#3995](https://github.com/ChainSafe/forest/pull/3995) Add
  `--p2p-listen-address` option to `forest` to override p2p addresses that
  forest listens on

- [#4031](https://github.com/ChainSafe/forest/pull/4031) Added RPC method
  `Filecoin.NetAgentVersion` and `--agent` flag to the `forest-cli net peers`
  subcommand, that will list the agent version of the connected peers.

- [#3955](https://github.com/ChainSafe/forest/pull/3955) Added support for the
  NV22 _Dragon_ network upgrade, together with the required state migration.

### Changed

- [#3976](https://github.com/ChainSafe/forest/pull/3976) `forest-wallet`
  defaults to using a local wallet instead of the builtin Forest wallet for
  greater security.

### Fixed

- [#4019](https://github.com/ChainSafe/forest/pull/4019) Fix Forest sending
  stale notifications after channel cancelation.

## Forest 0.16.8 "English Channel"

### Added

- [#3978](https://github.com/ChainSafe/forest/pull/3978) Add support for the
  `Filecoin.ChainNotify` RPC method.

## Forest 0.16.7 "Etaoin shrdlu"

Mandatory release that includes a fix for a bug in the `libp2p` usage. This is
necessary after the PL-managed bootstrap nodes were decommissioned. Failure to
upgrade will result in difficulty connecting to the mainnet network.

### Added

- [#3849](https://github.com/ChainSafe/forest/pull/3849/) Implement the
  `Filecoin.ChainGetPath` lotus-compatible RPC API.
- [#3849](https://github.com/ChainSafe/forest/pull/3849/) Add
  `forest-tool shed summarize-tipsets`.
- [#3893](https://github.com/ChainSafe/forest/pull/3983) Add
  `forest-tool shed peer-id-from-key-pair`.
- [#3981](https://github.com/ChainSafe/forest/issues/3981) Add
  `forest-tool backup create|restore`.

### Fixed

- [#3996](https://github.com/ChainSafe/forest/pull/3996) Fixed a bug in the
  `libp2p` usage that caused the connections to not get upgraded to secure ones.

## Forest 0.16.6 "Pinecone Reactivation"

### Added

- [#3866](https://github.com/ChainSafe/forest/pull/3866) Implement Offline RPC
  API.

### Fixed

- [#3857](https://github.com/ChainSafe/forest/pull/3907) Timeout parameter fetch
  to 30 minutes to avoid it getting stuck on IPFS gateway issues.
- [#3901](https://github.com/ChainSafe/forest/pull/3901) Fix timeout issue in
  `forest-cli snapshot export`.
- [#3919](https://github.com/ChainSafe/forest/pull/3919) Fix misreporting when
  logging progress.

## Forest 0.16.5 "Pinecone Deactivation"

Non-mandatory upgrade including mostly new RPC endpoints. The option to use an
alternative `FilOps` snapshot provider was removed given the service was
decommissioned.

### Added

- [#3817](https://github.com/ChainSafe/forest/pull/3817/) Implement the
  `Filecoin.StateVerifiedClientStatus` lotus-compatible RPC API.
- [#3824](https://github.com/ChainSafe/forest/pull/3824) Add `--ws` flag to
  `forest-tool api compare` to run all tests using WebSocket connections. Add
  support for WebSocket binary messages in Forest daemon.
- [#3802](https://github.com/ChainSafe/forest/pull/3802) Implement the
  `Filecoin.EthGetBalance` lotus-compatible RPC API.
- [#3773](https://github.com/ChainSafe/forest/pull/3811) Implement the
  `Filecoin.MpoolGetNonce` lotus-compatible RPC API.
- [#3773](https://github.com/ChainSafe/forest/pull/3786) Implement the
  `Filecoin.MinerGetBaseInfo` lotus-compatible RPC API.
- [#3807](https://github.com/ChainSafe/forest/pull/3807) Add `--run-ignored`
  flag to `forest-tool api compare`.
- [#3806](https://github.com/ChainSafe/forest/pull/3806) Implement the
  `Filecoin.EthGasPrice` lotus-compatible RPC API.

### Changed

- [#3819](https://github.com/ChainSafe/forest/pull/3819) Make progress messages
  more human-readable.
- [#3824](https://github.com/ChainSafe/forest/pull/3824) Demote noisy WebSocket
  info logs to debug in Forest daemon.

### Removed

- [#3878](https://github.com/ChainSafe/forest/issues/3878): FILOps is no longer
  serving lite snapshots. Removed `filops` option from
  `forest-tool snapshot fetch --vendor [vendor]`.

## Forest 0.16.4 "Speedy Gonzales"

### Breaking

### Added

- [#3779](https://github.com/ChainSafe/forest/pull/3779) Implement the
  `Filecoin.StateMinerRecoveries` lotus-compatible RPC API.
- [#3745](https://github.com/ChainSafe/forest/pull/3745) Implement the
  `Filecoin.StateCirculatingSupply` lotus-compatible RPC API.
- [#3773](https://github.com/ChainSafe/forest/pull/3773) Implement the
  `Filecoin.StateVMCirculatingSupplyInternal` lotus-compatible RPC API.
- [#3748](https://github.com/ChainSafe/forest/pull/3748) Add timing for each
  message and gas charge in the JSON output of
  `forest-tool snapshot compute-state` and `Filecoin.StateCall` RPC API.
- [#3720](https://github.com/ChainSafe/forest/pull/3750) Implement the
  `Filecoin.StateMinerInfo` lotus-compatible RPC API.
- [#1670](https://github.com/ChainSafe/forest/issues/1670) Support Butterflynet
  🦋.
- [#3801](https://github.com/ChainSafe/forest/pull/3801) Implement the
  `Filecoin.StateSearchMsg` lotus-compatible RPC API.
- [#3801](https://github.com/ChainSafe/forest/pull/3801) Implement the
  `Filecoin.StateSearchMsgLimited` lotus-compatible RPC API.

### Changed

### Removed

### Fixed

## Forest 0.16.3 "Tempura"

### Fixed

- [#3751](https://github.com/ChainSafe/forest/pull/3751) Workaround for
  performance bug that prevents Forest from syncing to the network.

## Forest 0.16.2 "November Rain"

### Breaking

### Added

- [#3749](https://github.com/ChainSafe/forest/pull/3749) Implement the
  `Filecoin.StateSectorGetInfo` lotus-compatible RPC API.
- [#3720](https://github.com/ChainSafe/forest/pull/3720) Implement the
  `Filecoin.GetParentMessages` lotus-compatible RPC API.
- [#3726](https://github.com/ChainSafe/forest/pull/3726) Implement the
  `Filecoin.StateMinerFaults` lotus-compatible RPC API.
- [#3735](https://github.com/ChainSafe/forest/pull/3735) Implement the
  `Filecoin.StateAccountKey` lotus-compatible RPC API.
- [#3744](https://github.com/ChainSafe/forest/pull/3744) Implement the
  `Filecoin.StateLookupID` lotus-compatible RPC API.
- [#3727](https://github.com/ChainSafe/forest/pull/3727) Added glif.io calibnet
  bootstrap node peer
- [#3737](https://github.com/ChainSafe/forest/pull/3737) Added `--n-tipsets`
  option to `forest-tool api compare`

### Changed

### Removed

### Fixed

## Forest 0.16.1 "(Re)Fresh(ed)Melon"

This is yet another mandatory upgrade for calibration network, containing the
2nd fix for the `WatermelonFix` upgrade. See this
[update](https://github.com/filecoin-project/community/discussions/74#discussioncomment-7591806)
for reference.

### Breaking

### Added

- [#3718](https://github.com/ChainSafe/forest/issues/3718) Added support for the
  2nd NV21 calibration network fix. See this
  [update](https://github.com/filecoin-project/community/discussions/74#discussioncomment-7591806)
  for details.

### Changed

### Removed

### Fixed

## Forest 0.16.0 "Rottenmelon"

This is a mandatory upgrade for calibration network, containing fix for the
`WatermelonFix` upgrade. See
[Lotus release](https://github.com/filecoin-project/lotus/releases/tag/v1.24.0-rc5)
for reference.

### Breaking

### Added

### Changed

- [#3072](https://github.com/ChainSafe/forest/issues/3072) Implemented
  mark-and-sweep GC, removing GC progress reports along with the corresponding
  RPC endpoint.

### Removed

### Fixed

- [#3540](https://github.com/ChainSafe/forest/issues/3540) Fix forest-cli sync
  wait to ensure that Forest is in the follow mode.
- [#3686](https://github.com/ChainSafe/forest/issues/3686) Fix regression when
  using `forest-tool db` subcommands and a `--chain` flag different from
  mainnet.

- [#3694](https://github.com/ChainSafe/forest/pull/3694) Calibration
  WatermelonFix recovery fix.

## Forest v0.15.2 "Defenestration"

### Breaking

### Added

- [#3632](https://github.com/ChainSafe/forest/issues/3632) Added an upgrade/fix
  for calibration network that will go live at epoch 1070494.

- [#3674](https://github.com/ChainSafe/forest/pull/3674) Added a tentative
  mainnet Watermelon upgrade with the
  [12.0.0-rc.2](https://github.com/filecoin-project/builtin-actors/releases/tag/v12.0.0-rc.2)
  bundle.

### Changed

### Removed

### Fixed

## Forest v0.15.1

Forest v0.15.1 is a service release with support for the v0.14.1 database.

### Breaking

### Added

- [#3662](https://github.com/ChainSafe/forest/pull/3662) Add `--filter` and
  `--fail-fast` flags to `forest-tool api compare`.
- [#3670](https://github.com/ChainSafe/forest/pull/3670) Implement the
  `Filecoin.ChainGetMessagesInTipset` lotus-compatible RPC API.

### Changed

### Removed

- [#3363](https://github.com/ChainSafe/forest/issues/3363) Remove hidden
  `forest-cli` commands used for helping users to migrate on `forest-tool` and
  `forest-wallet`.

### Fixed

## Forest v0.15.0 "Buttress"

Forest v0.15.0 is a service release containing minor bug fixes and small
usability improvements.

### Breaking

### Added

- [#3591](https://github.com/ChainSafe/forest/pull/3591) Add
  `forest-tool car validate` command for checking non-filecoin invariants in CAR
  files.
- [#3589](https://github.com/ChainSafe/forest/pull/3589) Add
  `forest-tool archive diff` command for debugging state-root mismatches.
- [#3609](https://github.com/ChainSafe/forest/pull/3609) Add `--no-metrics`
  option to `forest` for controlling the availability of the metrics Prometheus
  server.
- [#3613](https://github.com/ChainSafe/forest/pull/3613) Add `--expire-in`
  parameter to token commands.
- [#3584](https://github.com/ChainSafe/forest/issues/3584) Add
  `forest-tool api compare` command for testing RPC compatibility.

### Changed

- [#3614](https://github.com/ChainSafe/forest/issues/3614) Moved downloading
  bundle to runtime.

### Removed

- [#3589](https://github.com/ChainSafe/forest/pull/3589) Remove
  `forest-cli state diff` command. Replaced by `forest-tool archive diff`.
- [#3615](https://github.com/ChainSafe/forest/pull/3615) Remove `chain` section
  from forest configuration files.

### Fixed

- [#3619](https://github.com/ChainSafe/forest/pull/3619) Use correct timestamp
  in exported snapshot filenames.

## Forest v0.14.0 "Hakuna Matata"

### Breaking

### Added

- [#3422](https://github.com/ChainSafe/forest/issues/3422) Add NV21 (Watermelon)
  support for calibration network.
- [#3593](https://github.com/ChainSafe/forest/pull/3593): Add `--stateless` flag
  to `forest`. In stateless mode, forest connects to the P2P network but does
  not sync to HEAD.

### Changed

### Removed

### Fixed

- [#3590](https://github.com/ChainSafe/forest/pull/3590) Fix bug in ForestCAR
  encoder that would cause corrupted archives if a hash-collision happened.

## Forest v0.13.0 "Holocron"

### Breaking

- [#3231](https://github.com/ChainSafe/forest/issues/3231) Moved some Forest
  internal settings from files to the database.
- [#3333](https://github.com/ChainSafe/forest/pull/3333) Changed default rpc
  port from 1234 to 2345.
- [#3336](https://github.com/ChainSafe/forest/pull/3336) Moved following
  `forest-cli` subcommands to `forest-tool`
  - `archive info`
  - `fetch-params`
  - `snapshot fetch`
  - `snapshot validate`
- [#3355](https://github.com/ChainSafe/forest/pull/3355) Moved commands
  - `forest-cli db stats` to `forest-tool db stats`
  - `forest-cli db clean` to `forest-tool db destroy`
- [#3362](https://github.com/ChainSafe/forest/pull/3362) Moved the following
  `forest-cli wallet` subcommands to `forest-wallet`
- [#3432](https://github.com/ChainSafe/forest/pull/3432) Moved following
  `forest-cli` subcommands to `forest-tool`
  - `archive export`
  - `archive checkpoints`
- [#3431](https://github.com/ChainSafe/forest/pull/3431) Moved the following
  `forest-cli snapshot compress` subcommand to `forest-tool`
- [#3435](https://github.com/ChainSafe/forest/pull/3435) Moved subcommand
  `forest-cli car concat` subcommands to `forest-tool`

### Added

- [#3430](https://github.com/ChainSafe/forest/pull/3430): Add
  `forest-tool snapshot compute-state ...` subcommand.
- [#3321](https://github.com/ChainSafe/forest/issues/3321): Support for
  multi-threaded car-backed block stores.
- [#3316](https://github.com/ChainSafe/forest/pull/3316): Add
  `forest-tool benchmark` commands.
- [#3330](https://github.com/ChainSafe/forest/pull/3330): Add `--depth` flag to
  `forest-cli snapshot export`.
- [#3348](https://github.com/ChainSafe/forest/pull/3348): Add `--diff-depth`
  flag to `forest-cli archive export`.
- [#3325](https://github.com/ChainSafe/forest/pull/3325): Add
  `forest-tool state-migration actor-bundle` subcommand.
- [#3387](https://github.com/ChainSafe/forest/pull/3387): Add
  `forest-wallet delete` RPC command.
- [#3322](https://github.com/ChainSafe/forest/issues/3322): Added prompt to
  `forest-cli archive export` to overwrite file if the file specified with
  `--output-path` already exists and a `--force` flag to suppress the prompt.
- [#3439](https://github.com/ChainSafe/forest/pull/3439): Add
  `--consume-snapshot` option to `forest` command.
- [#3462](https://github.com/ChainSafe/forest/pull/3462): Add
  `forest-tool archive merge` command.

### Changed

- [#3331](https://github.com/ChainSafe/forest/pull/3331): Use multiple cores
  when exporting snapshots.
- [#3379](https://github.com/ChainSafe/forest/pull/3379): Improved state graph
  walking performance.
- [#3178](https://github.com/ChainSafe/forest/issues/3178): Removed inaccurate
  progress log ETA; now only the elapsed time is displayed.
- [#3322](https://github.com/ChainSafe/forest/issues/3322): The
  `snapshot export` and `snapshot compress` subcommands for `forest-cli` are now
  both consistent with `forest-cli archive export` in supporting a short-form
  output path flag `-o` and a long-form output path flag `--output-path`. The
  flag `--output` for the `snapshot compress` subcommand was replaced by
  `--output-path`.

### Removed

### Fixed

- [#3319](https://github.com/ChainSafe/forest/pull/3319): Fix bug triggered by
  re-encoding ForestCAR.zst files.

- [#3322](https://github.com/ChainSafe/forest/pull/3332): Forest is now able to
  parse data from epochs below 1_960_320 (on mainnet)

## Forest v0.12.1 "Carp++"

### Fixed

- [#3307](https://github.com/ChainSafe/forest/pull/3307)[#3310](https://github.com/ChainSafe/forest/pull/3310):
  Reduce memory requirements when exporting a snapshot by 50% (roughly from
  14GiB to 7GiB).

## Forest v0.12.0 "Carp"

Notable updates:

- Support for the `.forest.car.zst` format.
- Support for diff snapshots.

### Breaking

- [#3189](https://github.com/ChainSafe/forest/issues/3189): Changed the database
  organisation to use multiple columns. The database will need to be recreated.
- [#3220](https://github.com/ChainSafe/forest/pull/3220): Removed the
  `forest-cli chain validate-tipset-checkpoints` and
  `forest-cli chain tipset-hash` commands.

### Added

- [#3167](https://github.com/ChainSafe/forest/pull/3167): Added a new option
  `--validate-tipsets` for `forest-cli snapshot validate`.
- [#3166](https://github.com/ChainSafe/forest/issues/3166): Add
  `forest-cli archive info` command for inspecting archives.
- [#3159](https://github.com/ChainSafe/forest/issues/3159): Add
  `forest-cli archive export -e=X` command for exporting archives.
- [#3150](https://github.com/ChainSafe/forest/pull/3150):
  `forest-cli car concat` subcommand for concatenating `.car` files.
- [#3148](https://github.com/ChainSafe/forest/pull/3148): add `save_to_file`
  option to `forest-cli state fetch` command.
- [#3213](https://github.com/ChainSafe/forest/pull/3213): Add support for
  loading forest.car.zst files.
- [#3284](https://github.com/ChainSafe/forest/pull/3284): Add `--diff` flag to
  `archive export`.
- [#3292](https://github.com/ChainSafe/forest/pull/3292): Add `net info`
  subcommand to `forest-cli`.

### Changed

- [#3126](https://github.com/ChainSafe/forest/issues/3126): Bail on database
  lookup errors instead of silently ignoring them.
- [#2999](https://github.com/ChainSafe/forest/issues/2999): Restored `--tipset`
  flag to `forest-cli snapshot export` to allow export at a specific tipset.
- [#3283](https://github.com/ChainSafe/forest/pull/3283): All generated car
  files use the new forest.car.zst format.

### Removed

### Fixed

- [#3248](https://github.com/ChainSafe/forest/issues/3248): Fixed Forest being
  unable to re-create its libp2p keypair from file and always changing its
  `PeerId`.

## Forest v0.11.1 "Dagny Taggart"

## Forest v0.11.0 "Hypersonic"

### Breaking

- [#3048](https://github.com/ChainSafe/forest/pull/3048): Remove support for
  rocksdb
- [#3047](https://github.com/ChainSafe/forest/pull/3047): Remove support for
  compiling with delegated consensus
- [#3086](https://github.com/ChainSafe/forest/pull/3085):
  `forest-cli snapshot validate` no longer supports URLs. Download the snapshot
  and then run the command.

### Added

- [#2761](https://github.com/ChainSafe/forest/issues/2761): Add a per actor
  limit of 1000 messages to Forest mpool for preventing spam attacks.
- [#2728](https://github.com/ChainSafe/forest/issues/2728): Revive
  `forest-cli mpool pending` and `forest-cli mpool stat` subcommands.
- [#2816](https://github.com/ChainSafe/forest/issues/2816): Support `2k` devnet.
- [#3026](https://github.com/ChainSafe/forest/pull/3026): Expose
  `forest-cli state diff ...`
- [#3086](https://github.com/ChainSafe/forest/pull/3085):
  `forest-cli snapshot validate` is faster and uses less disk space, operating
  directly on the snapshot rather than loading through a database.
- [#2983](https://github.com/ChainSafe/forest/issues/2983): Added state
  migration support for NV17.
- [#3107](https://github.com/ChainSafe/forest/pull/3107): Introduced 'head'
  parameter for snapshot validation.

### Fixed

- [#3005](https://github.com/ChainSafe/forest/issues/3005): Fix incorrect
  progress reported when importing compressed snapshots.

- [#3122](https://github.com/ChainSafe/forest/pull/3122): Fix state-root
  mismatch around null tipsets.

## Forest v0.10.0 "Premature"

### Breaking

- [#3007](https://github.com/ChainSafe/forest/pull/3007): Optimize DB
  parameters. This requires all existing databases to be re-initialized.

### Fixed

- [#3006](https://github.com/ChainSafe/forest/issues/3006): Fix `premature end`
  error when exporting a snapshot.

## Forest v0.9.0 "Fellowship"

Notable updates:

- `--compressed` option removed from CLI, snapshots are now always compressed.
- The `dir`, `list`, `prune` and `remove` snapshot commands have been removed
  from the CLI.
- Snapshots are fetched to current directory by default.
- Added new subcommand `forest-cli info show`.
- `Filecoin.ChainSetHead` RPC endpoint and `forest-cli chain set-head`
  subcommand are now implemented.
- IPLD graph can now be downloaded via bitswap.
- `sendFIL` function has been updated to match recent changes in the Forest send
  command.
- FIL amount parsing/printing has been improved and 2 new options are added to
  forest-cli wallet list (--no-round and --no-abbrev).

### Breaking

- [#2873](https://github.com/ChainSafe/forest/issues/2873)
  - remove `--compressed` from the CLI. Snapshots are now always compressed.
  - Remove snapshot ops - snapshots fetched to the current directory by default.

### Added

- [#2706](https://github.com/ChainSafe/forest/issues/2706): implement
  `Filecoin.ChainSetHead` RPC endpoint and `forest-cli chain set-head`
  subcommand.
- [#2979](https://github.com/ChainSafe/forest/pull/2979): implement command for
  downloading an IPLD graph via bitswap.
- [#2578](https://github.com/ChainSafe/forest/pull/2578): implement initial
  support for `forest-cli info`

### Changed

- [#2668](https://github.com/ChainSafe/forest/issues/2668): JavaScript console
  `sendFIL` function has been updated to align with recent changes in the Forest
  `send` command (allowed units for the amount field are now "attoFIL",
  "femtoFIL", "picoFIL", "nanoFIL", "microFIL", "milliFIL", and "FIL"). Note
  that the default `sendFIL` amount unit (i.e., if no units are specified) is
  now FIL to match the behavior in Lotus.
- [#2833](https://github.com/ChainSafe/forest/issues/2833): Improvements to FIL
  amount parsing/printing, and add `--no-round` and `--no-abbrev` to
  `forest-cli wallet list`.

### Removed

- [#2888](https://github.com/ChainSafe/forest/issues/2888): FILOps is no longer
  serving uncompressed snapshots. Removed support for them in both `forest` and
  `forest-cli`.

### Fixed

- [#2967](https://github.com/ChainSafe/forest/issues/2967): Fix http-client
  concurrency issues caused by fetching root certificates multiple times.
- [#2958](https://github.com/ChainSafe/forest/issues/2958): Fix occasional
  consensus fault.
- [#2950](https://github.com/ChainSafe/forest/pull/2950): Fix cases where ctrl-c
  would be ignored.
- [#2934](https://github.com/ChainSafe/forest/issues/2934): Fix race condition
  when connecting to development blockchains.

## Forest v0.8.2 "The Way"

### Added

- [#2655](https://github.com/ChainSafe/forest/issues/2655): Configurable number
  of default recent state roots included in memory/snapshots.

### Changed

### Removed

### Fixed

- [#2796](https://github.com/ChainSafe/forest/pull/2796): Fix issue when running
  Forest on calibnet using a configuration file only.
- [#2807](https://github.com/ChainSafe/forest/pull/2807): Fix issue with v11
  actor CIDs.
- [#2804](https://github.com/ChainSafe/forest/pull/2804): Add work around for
  FVM bug that caused `forest-cli sync wait` to fail.

## Forest v0.8.1 "Cold Exposure"

### Fixed

- [#2788](https://github.com/ChainSafe/forest/pull/2788): Move back to the
  upstream `ref-fvm` and bump the dependency version so that it included the
  latest critical [patch](https://github.com/filecoin-project/ref-fvm/pull/1750)

## Forest v0.8.0 "Jungle Speed" (2023-04-21)

### Added

- [#2763](https://github.com/ChainSafe/forest/issues/2763): Support NV19 and
  NV20. ⛈️

## Forest v0.7.2 "Roberto" (2023-04-19)

### Added

- [#2741](https://github.com/ChainSafe/forest/issues/2741): Support importing
  zstd compressed snapshot car files
- [#2741](https://github.com/ChainSafe/forest/issues/2741): Support fetching
  zstd compressed snapshots with filecoin provider via `--compressed` option
- [#2741](https://github.com/ChainSafe/forest/issues/2741): Support exporting
  zstd compressed snapshots via `--compressed` option in
  `forest-cli snapshot export` subcommand
- [#1454](https://github.com/ChainSafe/forest/issues/1454): Added state
  migration support for NV18.

### Changed

- [#2770](https://github.com/ChainSafe/forest/issues/2767): Use `latest` tag for
  stable releases, and `edge` for latest development builds.

### Removed

### Fixed

## Forest v0.7.1 (2023-03-29)

Notable updates:

- Fix CD task for image publishing on new tagged releases

### Added

- [#2721](https://github.com/ChainSafe/forest/issues/2721): Add `--no-gc` flag
  to daemon.

### Changed

- [#2607](https://github.com/ChainSafe/forest/issues/2607): Use jemalloc as the
  default global allocator

### Removed

### Fixed

## Forest v0.7.0 (2023-03-23)

Notable updates:

- Support for NV18.
- Automatic database garbage collection.
- A JavaScript console to interact with Filecoin API.
- Switched to ParityDb as the default backend for Forest daemon.

### Added

- Support for NV18. [#2596](https://github.com/ChainSafe/forest/issues/2596)
- Automatic database garbage collection.
  [#2292](https://github.com/ChainSafe/forest/issues/2292)
  [#1708](https://github.com/ChainSafe/forest/issues/1708)
- ParityDb statistics to the stats endpoint.
  [#2433](https://github.com/ChainSafe/forest/issues/2433)
- A JavaScript console to interact with Filecoin API.
  [#2492](https://github.com/ChainSafe/forest/pull/2492)
- Multi-platform Docker image support.
  [#2476](https://github.com/ChainSafe/forest/issues/2476)
- `--dry-run` flag to forest-cli `snapshot export` command.
  [#2550](https://github.com/ChainSafe/forest/issues/2550)
- `--exit-after-init` and `--save-token` flags to daemon.
  [#2528](https://github.com/ChainSafe/forest/issues/2528)
- `--track-peak-rss` to forest daemon to get peak RSS usage.
  [#2696](https://github.com/ChainSafe/forest/pull/2696)
- RPC `Filecoin.Shutdown` endpoint and `forest-cli shutdown` subcommand.
  [#2402](https://github.com/ChainSafe/forest/issues/2402)
- Added retry capabilities to failing snapshot fetch.
  [#2544](https://github.com/ChainSafe/forest/issues/2544)

### Changed

- Network needs to be specified for most commands(eg Calibnet), including
  `sync wait` and `snapshot export`.
  [#2596](https://github.com/ChainSafe/forest/issues/2596)
- Switched to ParityDb as the default backend for Forest daemon. All clients
  must re-import the snapshot. The old database must be deleted manually - it is
  located in
  `$(forest-cli config dump | grep data_dir | cut -d' ' -f3)/<NETWORK>/rocksdb`.
  [#2576](https://github.com/ChainSafe/forest/issues/2576)
- Revised how balances are displayed, defaulting to:
  [#2323](https://github.com/ChainSafe/forest/issues/2323)
  - adding metric prefix when it's required, consequently CLI flag
    `--fixed-unit` added to force to show in original `FIL` unit
  - 4 significant digits, consequently CLI flag `--exact-balance` added to force
    full accuracy.
- `stats` and `compression` keys in `parity_db` section were renamed to
  `enable_statistics` and `compression_type` respectively.
  [#2433](https://github.com/ChainSafe/forest/issues/2433)
- `download_snapshot` key in `client` section configuration renamed to
  `auto_download_snapshot`.
  [#2457](https://github.com/ChainSafe/forest/pull/2457)
- `--skip-load` flag must be now called with a boolean indicating its value.
  [#2573](https://github.com/ChainSafe/forest/issues/2573)
- Ban peers with duration, Banned peers are automatically unbanned after a
  period of 1h. [#2391](https://github.com/ChainSafe/forest/issues/2391)
- Added support for multiple listen addr.
  [#2551](https://github.com/ChainSafe/forest/issues/2551)
- Allowed specifying the encryption passphrase via environmental variable.
  [#2499](https://github.com/ChainSafe/forest/issues/2499)
- Removed Forest `ctrl-c` hard shutdown behavior on subsequent `ctrl-c` signals.
  [#2538](https://github.com/ChainSafe/forest/pull/2538)
- Added support in the forest `send` command for all FIL units currently
  supported in forest `wallet` ("attoFIL", "femtoFIL", "picoFIL", "nanoFIL",
  "microFIL", "milliFIL", and "FIL"). Note that the default `send` units (i.e.,
  if no units are specified) are now FIL to match the behavior in Lotus.
  [#2668](https://github.com/ChainSafe/forest/issues/2668)

### Removed

- Removed `--halt-after-import` and `--auto-download-snapshot` from
  configuration. They are now strictly a CLI option.
  [#2528](https://github.com/ChainSafe/forest/issues/2528)
  [#2573](https://github.com/ChainSafe/forest/issues/2573)

### Fixed

- Daemon getting stuck in an infinite loop during shutdown.
  [#2672](https://github.com/ChainSafe/forest/issues/2672)
- `Scanning Blockchain` progess bar never hitting 100% during snapshot import.
  [#2404](https://github.com/ChainSafe/forest/issues/2404)
- bitswap queries cancellation that do not respond after a period.
  [#2398](https://github.com/ChainSafe/forest/issues/2398)
- Forest daeamon crashing on sending bitswap requests.
  [#2405](https://github.com/ChainSafe/forest/issues/2405)
- Corrected counts displayed when using `forest-cli --chain <chain> sync wait`.
  [#2429](https://github.com/ChainSafe/forest/issues/2429)
- Snapshot export issue when running on a system with a separate temporary
  filesystem. [#2693](https://github.com/ChainSafe/forest/pull/2693)
- All binaries and crates in the project to follow a standard version, based on
  the release tag. [#2493](https://github.com/ChainSafe/forest/issues/2493)

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
- `f5fe14d2` [Audit fixes] FOR-03 - Inconsistent Deserialization of Randomness ([#1205](https://github.com/ChainSafe/forest/pull/1205))
  (Hunter Trujillo)
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
- `f698ba88` [Audit fixes] FOR-02: Inconsistent Deserialization of Address ID ([#1149](https://github.com/ChainSafe/forest/pull/1149))
  (Hunter Trujillo)
- `e50d2ae8` [Audit fixes] FOR-16: Unnecessary Extensive Permissions for Private
  Keys ([#1151](https://github.com/ChainSafe/forest/pull/1151)) (Hunter Trujillo)
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
