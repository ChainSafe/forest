---
title: Command Line Options
sidebar_position: 1
---

<!--
CLI reference documentation for forest, forest-wallet, forest-cli, and forest-tool.
Do not edit manually, use the `generate_cli_md.sh` script.
-->

This document lists every command line option and sub-command for Forest.

## `forest`

```
forest-filecoin 0.31.1
ChainSafe Systems <info@chainsafe.io>
Rust Filecoin implementation.

USAGE:
  forest [OPTIONS]

SUBCOMMANDS:


OPTIONS:
      --config <CONFIG>
          A TOML file containing relevant configurations
      --genesis <GENESIS>
          The genesis CAR file
      --rpc <RPC>
          Allow RPC to be active or not (default: true) [possible values: true, false]
      --no-metrics
          Disable Metrics endpoint
      --metrics-address <METRICS_ADDRESS>
          Address used for metrics collection server. By defaults binds on localhost on port 6116
      --rpc-address <RPC_ADDRESS>
          Address used for RPC. By defaults binds on localhost on port 2345
      --rpc-filter-list <RPC_FILTER_LIST>
          Path to a list of RPC methods to allow/disallow
      --no-healthcheck
          Disable healthcheck endpoints
      --healthcheck-address <HEALTHCHECK_ADDRESS>
          Address used for healthcheck server. By defaults binds on localhost on port 2346
      --p2p-listen-address <P2P_LISTEN_ADDRESS>
          P2P listen addresses, e.g., `--p2p-listen-address /ip4/0.0.0.0/tcp/12345 --p2p-listen-address /ip4/0.0.0.0/tcp/12346`
      --kademlia <KADEMLIA>
          Allow Kademlia (default: true) [possible values: true, false]
      --mdns <MDNS>
          Allow MDNS (default: false) [possible values: true, false]
      --height <HEIGHT>
          Validate snapshot at given EPOCH, use a negative value -N to validate the last N EPOCH(s) starting at HEAD
      --head <HEAD>
          Sets the current HEAD epoch to validate to. Useful to specify a smaller range in conjunction with `height`, ignored if `height` is unspecified
      --import-snapshot <IMPORT_SNAPSHOT>
          Import a snapshot from a local CAR file or URL
      --import-mode <IMPORT_MODE>
          Snapshot import mode. Available modes are `auto`, `copy`, `move`, `symlink` and `hardlink` [default: auto]
      --halt-after-import
          Halt with exit code 0 after successfully importing a snapshot
      --skip-load <SKIP_LOAD>
          Skips loading CAR file and uses header to index chain. Assumes a pre-loaded database [possible values: true, false]
      --req-window <REQ_WINDOW>
          Number of tipsets requested over one chain exchange (default is 8)
      --tipset-sample-size <TIPSET_SAMPLE_SIZE>
          Number of tipsets to include in the sample that determines what the network head is (default is 5)
      --target-peer-count <TARGET_PEER_COUNT>
          Amount of Peers we want to be connected to (default is 75)
      --encrypt-keystore <ENCRYPT_KEYSTORE>
          Encrypt the key-store (default: true) [possible values: true, false]
      --chain <CHAIN>
          Choose network chain to sync to
      --auto-download-snapshot
          Automatically download a chain specific snapshot to sync with the Filecoin network if needed
      --color <COLOR>
          Enable or disable colored logging in `stdout` [default: auto]
      --tokio-console
          Turn on tokio-console support for debugging
      --loki
          Send telemetry to `grafana loki`
      --loki-endpoint <LOKI_ENDPOINT>
          Endpoint of `grafana loki` [default: http://127.0.0.1:3100]
      --log-dir <LOG_DIR>
          Specify a directory into which rolling log files should be appended
      --exit-after-init
          Exit after basic daemon initialization
      --save-token <SAVE_TOKEN>
          If provided, indicates the file to which to save the admin token
      --no-gc
          Disable the automatic database garbage collection
      --stateless
          In stateless mode, forest connects to the P2P network but does not sync to HEAD
      --dry-run
          Check your command-line options and configuration file if one is used
      --skip-load-actors
          Skip loading actors from the actors bundle
  -h, --help
          Print help
  -V, --version
          Print version
```

## `forest-wallet`

```
forest-filecoin 0.31.1
ChainSafe Systems <info@chainsafe.io>
Rust Filecoin implementation.

USAGE:
  forest-wallet [OPTIONS] <COMMAND>

SUBCOMMANDS:
  new               Create a new wallet
  balance           Get account balance
  default           Get the default address of the wallet
  export            Export the wallet's keys
  has               Check if the wallet has a key
  import            Import keys from existing wallet
  list              List addresses of the wallet
  set-default       Set the default wallet address
  sign              Sign a message
  validate-address  Validates whether a given string can be decoded as a well-formed address
  verify            Verify the signature of a message. Returns true if the signature matches the message and address
  delete            Deletes the wallet associated with the given address
  send              Send funds between accounts
  help              Print this message or the help of the given subcommand(s)

OPTIONS:
      --token <TOKEN>  Admin token to interact with the node
      --remote-wallet  Use remote wallet associated with the Filecoin node. Warning! You should ensure that your connection is encrypted and secure, as the communication between the wallet and the node is **not** encrypted
      --encrypt        Encrypt local wallet
  -h, --help           Print help
  -V, --version        Print version
```

### `forest-wallet new`

```
Create a new wallet

Usage: forest-wallet new [SIGNATURE_TYPE]

Arguments:
  [SIGNATURE_TYPE]  The signature type to use. One of `secp256k1`, `bls` or `delegated` [default: secp256k1]

Options:
  -h, --help  Print help
```

### `forest-wallet balance`

```
Get account balance

Usage: forest-wallet balance [OPTIONS] <ADDRESS>

Arguments:
  <ADDRESS>  The address of the account to check

Options:
      --no-round   Output is rounded to 4 significant figures by default. Do not round
      --no-abbrev  Output may be given an SI prefix like `atto` by default. Do not do this, showing whole FIL at all times
  -h, --help       Print help
```

### `forest-wallet default`

```
Get the default address of the wallet

Usage: forest-wallet default

Options:
  -h, --help  Print help
```

### `forest-wallet export`

```
Export the wallet's keys

Usage: forest-wallet export <ADDRESS>

Arguments:
  <ADDRESS>  The address that contains the keys to export

Options:
  -h, --help  Print help
```

### `forest-wallet has`

```
Check if the wallet has a key

Usage: forest-wallet has <KEY>

Arguments:
  <KEY>  The key to check

Options:
  -h, --help  Print help
```

### `forest-wallet import`

```
Import keys from existing wallet

Usage: forest-wallet import [PATH]

Arguments:
  [PATH]  The path to the private key

Options:
  -h, --help  Print help
```

### `forest-wallet list`

```
List addresses of the wallet

Usage: forest-wallet list [OPTIONS]

Options:
      --no-round   Output is rounded to 4 significant figures by default. Do not round
      --no-abbrev  Output may be given an SI prefix like `atto` by default. Do not do this, showing whole FIL at all times
  -h, --help       Print help
```

### `forest-wallet set-default`

```
Set the default wallet address

Usage: forest-wallet set-default <KEY>

Arguments:
  <KEY>  The given key to set to the default address

Options:
  -h, --help  Print help
```

### `forest-wallet sign`

```
Sign a message

Usage: forest-wallet sign -m <MESSAGE> -a <ADDRESS>

Options:
  -m <MESSAGE>  The hex encoded message to sign
  -a <ADDRESS>  The address to be used to sign the message
  -h, --help    Print help
```

### `forest-wallet validate-address`

```
Validates whether a given string can be decoded as a well-formed address

Usage: forest-wallet validate-address <ADDRESS>

Arguments:
  <ADDRESS>  The address to be validated

Options:
  -h, --help  Print help
```

### `forest-wallet verify`

```
Verify the signature of a message. Returns true if the signature matches the message and address

Usage: forest-wallet verify -a <ADDRESS> -m <MESSAGE> -s <SIGNATURE>

Options:
  -a <ADDRESS>    The address used to sign the message
  -m <MESSAGE>    The message to verify
  -s <SIGNATURE>  The signature of the message to verify
  -h, --help      Print help
```

### `forest-wallet delete`

```
Deletes the wallet associated with the given address

Usage: forest-wallet delete <ADDRESS>

Arguments:
  <ADDRESS>  The address of the wallet to delete

Options:
  -h, --help  Print help
```

### `forest-wallet send`

```
Send funds between accounts

Usage: forest-wallet send [OPTIONS] <TARGET_ADDRESS> <AMOUNT>

Arguments:
  <TARGET_ADDRESS>
  <AMOUNT>

Options:
      --from <FROM>                optionally specify the account to send funds from (otherwise the default one will be used)
      --gas-feecap <GAS_FEECAP>    [default: 0.0]
      --gas-limit <GAS_LIMIT>      In milliGas [default: 0]
      --gas-premium <GAS_PREMIUM>  [default: 0.0]
  -h, --help                       Print help
```

## `forest-cli`

```
forest-filecoin 0.31.1
ChainSafe Systems <info@chainsafe.io>
Rust Filecoin implementation.

USAGE:
  forest-cli [OPTIONS] <COMMAND>

SUBCOMMANDS:
  chain        Interact with Filecoin blockchain
  auth         Manage RPC permissions
  net          Manage P2P network
  sync         Inspect or interact with the chain synchronizer
  mpool        Interact with the message pool
  state        Interact with and query Filecoin chain state
  config       Manage node configuration
  snapshot     Manage snapshots
  info         Print node info
  shutdown     Shutdown Forest
  healthcheck  Print healthcheck info
  f3           Manages Filecoin Fast Finality (F3) interactions
  wait-api     Wait for lotus API to come online
  help         Print this message or the help of the given subcommand(s)

OPTIONS:
  -t, --token <TOKEN>  Client JWT token to use for JSON-RPC authentication
  -h, --help           Print help
  -V, --version        Print version
```

### `forest-cli chain`

```
Interact with Filecoin blockchain

Usage: forest-cli chain <COMMAND>

Commands:
  block     Retrieves and prints out the block specified by the given CID
  genesis   Prints out the genesis tipset
  head      Prints out the canonical head of the chain
  message   Reads and prints out a message referenced by the specified CID from the chain block store
  read-obj  Reads and prints out IPLD nodes referenced by the specified CID from chain block store and returns raw bytes
  set-head  Manually set the head to the given tipset. This invalidates blocks between the desired head and the new head
  prune     Prune chain database
  list      View a segment of the chain
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli chain block`

```
Retrieves and prints out the block specified by the given CID

Usage: forest-cli chain block -c <CID>

Options:
  -c <CID>
  -h, --help  Print help
```

### `forest-cli chain message`

```
Reads and prints out a message referenced by the specified CID from the chain block store

Usage: forest-cli chain message -c <CID>

Options:
  -c <CID>
  -h, --help  Print help
```

### `forest-cli chain read-obj`

```
Reads and prints out IPLD nodes referenced by the specified CID from chain block store and returns raw bytes

Usage: forest-cli chain read-obj -c <CID>

Options:
  -c <CID>
  -h, --help  Print help
```

### `forest-cli chain set-head`

```
Manually set the head to the given tipset. This invalidates blocks between the desired head and the new head

Usage: forest-cli chain set-head [OPTIONS] <CIDS>...

Arguments:
  <CIDS>...  Construct the new head tipset from these CIDs

Options:
      --epoch <EPOCH>  Use the tipset from this epoch as the new head. Negative numbers specify decrements from the current head
  -f, --force          Skip confirmation dialogue
  -h, --help           Print help
```

### `forest-cli chain prune`

```
Prune chain database

Usage: forest-cli chain prune <COMMAND>

Commands:
  snap  Run snapshot GC
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli chain list`

```
View a segment of the chain

Usage: forest-cli chain list [OPTIONS]

Options:
      --epoch <EPOCH>  Start epoch (default: current head)
      --count <COUNT>  Number of tipsets [default: 30]
      --gas-stats      View gas statistics for the chain
  -h, --help           Print help
```

### `forest-cli auth`

```
Manage RPC permissions

Usage: forest-cli auth <COMMAND>

Commands:
  create-token  Create a new Authentication token with given permission
  api-info      Get RPC API Information
  help          Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli auth create-token`

```
Create a new Authentication token with given permission

Usage: forest-cli auth create-token [OPTIONS] --perm <PERM>

Options:
  -p, --perm <PERM>            Permission to assign to the token, one of: read, write, sign, admin
      --expire-in <EXPIRE_IN>  Token is revoked after this duration [default: "2 months"]
  -h, --help                   Print help
```

### `forest-cli auth api-info`

```
Get RPC API Information

Usage: forest-cli auth api-info [OPTIONS] --perm <PERM>

Options:
  -p, --perm <PERM>            permission to assign the token, one of: read, write, sign, admin
      --expire-in <EXPIRE_IN>  Token is revoked after this duration [default: "2 months"]
  -h, --help                   Print help
```

### `forest-cli net`

```
Manage P2P network

Usage: forest-cli net <COMMAND>

Commands:
  listen        Lists `libp2p` swarm listener addresses
  info          Lists `libp2p` swarm network info
  peers         Lists `libp2p` swarm peers
  connect       Connects to a peer by its peer ID and multi-addresses
  disconnect    Disconnects from a peer by it's peer ID
  reachability  Print information about reachability from the internet
  help          Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli net peers`

```
Lists `libp2p` swarm peers

Usage: forest-cli net peers [OPTIONS]

Options:
  -a, --agent  Print agent name
  -h, --help   Print help
```

### `forest-cli net connect`

```
Connects to a peer by its peer ID and multi-addresses

Usage: forest-cli net connect <ADDRESS>

Arguments:
  <ADDRESS>  Multi-address (with `/p2p/` protocol)

Options:
  -h, --help  Print help
```

### `forest-cli net disconnect`

```
Disconnects from a peer by it's peer ID

Usage: forest-cli net disconnect <ID>

Arguments:
  <ID>  Peer ID to disconnect from

Options:
  -h, --help  Print help
```

### `forest-cli sync`

```
Inspect or interact with the chain synchronizer

Usage: forest-cli sync <COMMAND>

Commands:
  wait       Display continuous sync data until sync is complete
  status     Check sync status
  check-bad  Check if a given block is marked bad, and for what reason
  mark-bad   Mark a given block as bad
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli sync wait`

```
Display continuous sync data until sync is complete

Usage: forest-cli sync wait [OPTIONS]

Options:
  -w          Don't exit after node is synced
  -h, --help  Print help
```

### `forest-cli sync check-bad`

```
Check if a given block is marked bad, and for what reason

Usage: forest-cli sync check-bad -c <CID>

Options:
  -c <CID>    The block CID to check
  -h, --help  Print help
```

### `forest-cli sync mark-bad`

```
Mark a given block as bad

Usage: forest-cli sync mark-bad -c <CID>

Options:
  -c <CID>    The block CID to mark as a bad block
  -h, --help  Print help
```

### `forest-cli mpool`

```
Interact with the message pool

Usage: forest-cli mpool <COMMAND>

Commands:
  pending  Get pending messages
  nonce    Get the current nonce for an address
  stat     Print mempool stats
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli mpool pending`

```
Get pending messages

Usage: forest-cli mpool pending [OPTIONS]

Options:
      --local        Print pending messages for addresses in local wallet only
      --cids         Only print `CIDs` of messages in output
      --to <TO>      Return messages to a given address
      --from <FROM>  Return messages from a given address
  -h, --help         Print help
```

### `forest-cli mpool stat`

```
Print mempool stats

Usage: forest-cli mpool stat [OPTIONS]

Options:
      --basefee-lookback <BASEFEE_LOOKBACK>
          Number of blocks to look back for minimum `basefee` [default: 60]
      --local
          Print stats for addresses in local wallet only
  -h, --help
          Print help
```

### `forest-cli mpool nonce`

```
Get the current nonce for an address

Usage: forest-cli mpool nonce <ADDRESS>

Arguments:
  <ADDRESS>  Address to check nonce for

Options:
  -h, --help  Print help
```

### `forest-cli state`

```
Interact with and query Filecoin chain state

Usage: forest-cli state <COMMAND>

Commands:
  fetch
  compute     Compute state trees for epochs
  read-state  Read the state of an actor
  actor-cids  Returns the built-in actor bundle CIDs for the current network
  help        Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli state fetch`

```
Usage: forest-cli state fetch [OPTIONS] <ROOT>

Arguments:
  <ROOT>

Options:
  -s, --save-to-file <SAVE_TO_FILE>  The `.car` file path to save the state root
  -h, --help                         Print help
```

### `forest-cli state compute`

```
Compute state trees for epochs

Usage: forest-cli state compute [OPTIONS] --epoch <EPOCH>

Options:
      --epoch <EPOCH>        Which epoch to compute the state transition for
  -n, --n-epochs <N_EPOCHS>  Number of tipset epochs to compute state for. Default is 1
  -v, --verbose              Print epoch and tipset key along with state root
  -h, --help                 Print help
```

### `forest-cli config`

```
Manage node configuration

Usage: forest-cli config <COMMAND>

Commands:
  dump  Dump default configuration to standard output
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli snapshot`

```
Manage snapshots

Usage: forest-cli snapshot <COMMAND>

Commands:
  export         Export a snapshot of the chain to `<output_path>`
  export-status  Show status of the current export
  export-cancel  Cancel the current export
  export-diff    Export a diff snapshot between `from` and `to` epochs to `<output_path>`
  help           Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli snapshot export`

```
Export a snapshot of the chain to `<output_path>`

Usage: forest-cli snapshot export [OPTIONS]

Options:
  -o, --output-path <OUTPUT_PATH>  `./forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.zst`. [default: .]
      --skip-checksum              Skip creating the checksum file
      --dry-run                    Don't write the archive
  -t, --tipset <TIPSET>            Tipset to start the export from, default is the chain head
  -d, --depth <DEPTH>              How many state trees to include. 0 for chain spine with no state trees [default: 2000]
      --format <FORMAT>            Snapshot format to export [default: v2] [possible values: v1, v2]
  -h, --help                       Print help
```

### `forest-cli send`

```

```

### `forest-cli info`

```
Print node info

Usage: forest-cli info <COMMAND>

Commands:
  show
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli shutdown`

```
Shutdown Forest

Usage: forest-cli shutdown [OPTIONS]

Options:
      --force  Assume "yes" as answer to shutdown prompt
  -h, --help   Print help
```

### `forest-cli healthcheck`

```
Print healthcheck info

Usage: forest-cli healthcheck <COMMAND>

Commands:
  ready    Display readiness status
  live     Display liveness status
  healthy  Display health status
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli healthcheck ready`

```
Display readiness status

Usage: forest-cli healthcheck ready [OPTIONS]

Options:
      --wait                                 Don't exit until node is ready
      --healthcheck-port <HEALTHCHECK_PORT>  Healthcheck port [default: 2346]
  -h, --help                                 Print help
```

### `forest-cli f3`

```
Manages Filecoin Fast Finality (F3) interactions

Usage: forest-cli f3 <COMMAND>

Commands:
  manifest    Gets the current manifest used by F3
  status      Checks the F3 status
  certs       Manages interactions with F3 finality certificates [aliases: c]
  powertable  Gets F3 power table at a specific instance ID or latest instance if none is specified [aliases: pt]
  ready       Checks if F3 is in sync
  help        Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli f3 manifest`

```
Gets the current manifest used by F3

Usage: forest-cli f3 manifest [OPTIONS]

Options:
      --output <OUTPUT>
          The output format

          Possible values:
          - text: Text
          - json: JSON

          [default: text]

  -h, --help
          Print help (see a summary with '-h')
```

### `forest-cli f3 status`

```
Checks the F3 status

Usage: forest-cli f3 status

Options:
  -h, --help  Print help
```

### `forest-cli f3 certs`

```
Manages interactions with F3 finality certificates

Usage: forest-cli f3 certs <COMMAND>

Commands:
  get   Gets an F3 finality certificate to a given instance ID, or the latest certificate if no instance is specified
  list  Lists a range of F3 finality certificates
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli f3 certs get`

```
Gets an F3 finality certificate to a given instance ID, or the latest certificate if no instance is specified

Usage: forest-cli f3 certs get [OPTIONS] [INSTANCE]

Arguments:
  [INSTANCE]


Options:
      --output <OUTPUT>
          The output format

          Possible values:
          - text: Text
          - json: JSON

          [default: text]

  -h, --help
          Print help (see a summary with '-h')
```

### `forest-cli f3 certs list`

```
Lists a range of F3 finality certificates

Usage: forest-cli f3 certs list [OPTIONS] [RANGE]

Arguments:
  [RANGE]
          Inclusive range of `from` and `to` instances in following notation: `<from>..<to>`. Either `<from>` or `<to>` may be omitted, but not both

Options:
      --output <OUTPUT>
          The output format

          Possible values:
          - text: Text
          - json: JSON

          [default: text]

      --limit <LIMIT>
          The maximum number of instances. A value less than 0 indicates no limit

          [default: 10]

      --reverse
          Reverses the default order of output

  -h, --help
          Print help (see a summary with '-h')
```

### `forest-cli f3 powertable`

```
Gets F3 power table at a specific instance ID or latest instance if none is specified

Usage: forest-cli f3 powertable <COMMAND>

Commands:
  get             Gets F3 power table at a specific instance ID or latest instance if none is specified [aliases: g]
  get-proportion  Gets the total proportion of power for a list of actors at a given instance [aliases: gp]
  help            Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-cli f3 powertable get`

```
Gets F3 power table at a specific instance ID or latest instance if none is specified

Usage: forest-cli f3 powertable get [OPTIONS] [INSTANCE]

Arguments:
  [INSTANCE]  instance ID. (default: latest)

Options:
      --ec    Whether to get the power table from EC. (default: false)
  -h, --help  Print help
```

### `forest-cli f3 powertable get-proportion`

```
Gets the total proportion of power for a list of actors at a given instance

Usage: forest-cli f3 powertable get-proportion [OPTIONS] [ACTOR_IDS]...

Arguments:
  [ACTOR_IDS]...

Options:
      --instance <INSTANCE>  instance ID. (default: latest)
      --ec                   Whether to get the power table from EC. (default: false)
  -h, --help                 Print help
```

### `forest-cli f3 ready`

```
Checks if F3 is in sync

Usage: forest-cli f3 ready [OPTIONS]

Options:
      --wait
          Wait until F3 is in sync
      --threshold <THRESHOLD>
          The threshold of the epoch gap between chain head and F3 head within which F3 is considered in sync [default: 20]
      --no-progress-timeout <NO_PROGRESS_TIMEOUT>
          Exit after F3 making no progress for this duration [default: 10m]
  -h, --help
          Print help
```

## `forest-tool`

```
forest-filecoin 0.31.1
ChainSafe Systems <info@chainsafe.io>
Rust Filecoin implementation.

USAGE:
  forest-tool <COMMAND>

SUBCOMMANDS:
  backup           Create and restore backups
  benchmark        Benchmark various Forest subsystems
  state-migration  State migration tools
  snapshot         Manage snapshots
  fetch-params     Download parameters for generating and verifying proofs for given size
  archive          Manage archives
  db               Database management
  index            Index database management
  car              Utilities for manipulating CAR files
  api              API tooling
  net              Network utilities
  shed             Miscellaneous, semver-exempt commands for developer use
  completion       Completion Command for generating shell completions for the CLI
  help             Print this message or the help of the given subcommand(s)

OPTIONS:
  -h, --help     Print help
  -V, --version  Print version
```

### `forest-tool backup`

```
Create and restore backups

Usage: forest-tool backup <COMMAND>

Commands:
  create   Create a backup of the node. By default, only the peer-to-peer key-pair and key-store are backed up. The node must be offline
  restore  Restore a backup of the node from a file. The node must be offline
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-tool backup create`

```
Create a backup of the node. By default, only the peer-to-peer key-pair and key-store are backed up. The node must be offline

Usage: forest-tool backup create [OPTIONS]

Options:
      --backup-file <BACKUP_FILE>      Path to the output backup file if not using the default
      --all                            Backup everything from the Forest data directory. This will override other options
      --no-keypair                     Disables backing up the key-pair
      --no-keystore                    Disables backing up the key-store
      --backup-chain <BACKUP_CHAIN>    Backs up the blockstore for the specified chain. If not provided, it will not be backed up
      --include-proof-params           Include proof parameters in the backup
  -d, --daemon-config <DAEMON_CONFIG>  Optional TOML file containing forest daemon configuration. If not provided, the default configuration will be used
  -h, --help                           Print help
```

### `forest-tool backup restore`

```
Restore a backup of the node from a file. The node must be offline

Usage: forest-tool backup restore [OPTIONS] <BACKUP_FILE>

Arguments:
  <BACKUP_FILE>  Path to the backup file

Options:
  -d, --daemon-config <DAEMON_CONFIG>  Optional TOML file containing forest daemon configuration. If not provided, the default configuration will be used
      --force                          Force restore even if files already exist WARNING: This will overwrite existing files
  -h, --help                           Print help
```

### `forest-tool completion`

```
Completion Command for generating shell completions for the CLI

Usage: forest-tool completion [OPTIONS] [BINARIES]...

Arguments:
  [BINARIES]...  The binaries for which to generate completions (e.g., 'forest-cli,forest-tool,forest-wallet'). If omitted, completions for all known binaries will be generated

Options:
      --shell <SHELL>  The Shell type to generate completions for [default: bash] [possible values: bash, elvish, fish, powershell, zsh]
  -h, --help           Print help
```

### `forest-tool benchmark`

```
Benchmark various Forest subsystems

Usage: forest-tool benchmark <COMMAND>

Commands:
  car-streaming    Benchmark streaming data from a CAR archive
  graph-traversal  Depth-first traversal of the Filecoin graph
  forest-encoding  Encoding of a `.forest.car.zst` file
  export           Exporting a `.forest.car.zst` file from HEAD
  blockstore       Benchmark key-value blockstore
  help             Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-tool benchmark car-streaming`

```
Benchmark streaming data from a CAR archive

Usage: forest-tool benchmark car-streaming [OPTIONS] <SNAPSHOT_FILES>...

Arguments:
  <SNAPSHOT_FILES>...  Snapshot input files (`.car.`, `.car.zst`, `.forest.car.zst`)

Options:
      --inspect  Whether or not we want to expect [`ipld_core::ipld::Ipld`] data for each block
  -h, --help     Print help
```

### `forest-tool benchmark graph-traversal`

```
Depth-first traversal of the Filecoin graph

Usage: forest-tool benchmark graph-traversal <SNAPSHOT_FILES>...

Arguments:
  <SNAPSHOT_FILES>...  Snapshot input files (`.car.`, `.car.zst`, `.forest.car.zst`)

Options:
  -h, --help  Print help
```

### `forest-tool benchmark forest-encoding`

```
Encoding of a `.forest.car.zst` file

Usage: forest-tool benchmark forest-encoding [OPTIONS] <SNAPSHOT_FILE>

Arguments:
  <SNAPSHOT_FILE>  Snapshot input file (`.car.`, `.car.zst`, `.forest.car.zst`)

Options:
      --compression-level <COMPRESSION_LEVEL>
          [default: 3]
      --frame-size <FRAME_SIZE>
          End zstd frames after they exceed this length [default: 8192]
  -h, --help
          Print help
```

### `forest-tool benchmark export`

```
Exporting a `.forest.car.zst` file from HEAD

Usage: forest-tool benchmark export [OPTIONS] <SNAPSHOT_FILES>...

Arguments:
  <SNAPSHOT_FILES>...  Snapshot input files (`.car.`, `.car.zst`, `.forest.car.zst`)

Options:
      --compression-level <COMPRESSION_LEVEL>
          [default: 3]
      --frame-size <FRAME_SIZE>
          End zstd frames after they exceed this length [default: 8192]
  -e, --epoch <EPOCH>
          Latest epoch that has to be exported for this snapshot, the upper bound. This value cannot be greater than the latest epoch available in the input snapshot
  -d, --depth <DEPTH>
          How many state-roots to include. Lower limit is 900 for `calibnet` and `mainnet` [default: 2000]
  -h, --help
          Print help
```

### `forest-tool state-migration`

```
State migration tools

Usage: forest-tool state-migration <COMMAND>

Commands:
  actor-bundle              Generate a merged actor bundle from the hard-coded sources in forest
  generate-actors-metadata  Generate actors metadata from required bundles list
  help                      Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-tool state-migration actor-bundle`

```
Generate a merged actor bundle from the hard-coded sources in forest

Usage: forest-tool state-migration actor-bundle [OUTPUT]

Arguments:
  [OUTPUT]  [default: actor_bundles.car.zst]

Options:
  -h, --help  Print help
```

### `forest-tool snapshot`

```
Manage snapshots

Usage: forest-tool snapshot <COMMAND>

Commands:
  fetch           Fetches the most recent snapshot from a trusted, pre-defined location
  validate-diffs  Validate the provided snapshots as a whole
  validate        Validate the snapshots individually
  compress        Make this snapshot suitable for use as a compressed car-backed blockstore
  compute-state   Compute the state hash at a given epoch
  help            Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-tool snapshot fetch`

```
Fetches the most recent snapshot from a trusted, pre-defined location

Usage: forest-tool snapshot fetch [OPTIONS]

Options:
  -d, --directory <DIRECTORY>  [default: .]
      --chain <CHAIN>          Network chain the snapshot will belong to [default: mainnet]
  -v, --vendor <VENDOR>        Vendor to fetch the snapshot from [default: forest] [possible values: forest]
  -h, --help                   Print help
```

### `forest-tool snapshot validate-diffs`

```
Validate the provided snapshots as a whole

Usage: forest-tool snapshot validate-diffs [OPTIONS] <SNAPSHOT_FILES>...

Arguments:
  <SNAPSHOT_FILES>...  Path to a snapshot CAR, which may be zstd compressed

Options:
      --check-links <CHECK_LINKS>
          Number of recent epochs to scan for broken links [default: 2000]
      --check-network <CHECK_NETWORK>
          Assert the snapshot belongs to this network. If left blank, the network will be inferred before executing messages
      --check-stateroots <CHECK_STATEROOTS>
          Number of recent epochs to scan for bad messages/transactions [default: 60]
  -h, --help
          Print help
```

### `forest-tool snapshot validate`

```
Validate the snapshots individually

Usage: forest-tool snapshot validate [OPTIONS] <SNAPSHOT_FILES>...

Arguments:
  <SNAPSHOT_FILES>...  Path to a snapshot CAR, which may be zstd compressed

Options:
      --check-links <CHECK_LINKS>
          Number of recent epochs to scan for broken links [default: 2000]
      --check-network <CHECK_NETWORK>
          Assert the snapshot belongs to this network. If left blank, the network will be inferred before executing messages
      --check-stateroots <CHECK_STATEROOTS>
          Number of recent epochs to scan for bad messages/transactions [default: 60]
      --fail-fast
          Fail at the first invalid snapshot
  -h, --help
          Print help
```

### `forest-tool snapshot compress`

```
Make this snapshot suitable for use as a compressed car-backed blockstore

Usage: forest-tool snapshot compress [OPTIONS] <SOURCE>

Arguments:
  <SOURCE>
          Input CAR file, in `.car`, `.car.zst`, or `.forest.car.zst` format

Options:
  -o, --output-path <OUTPUT_PATH>
          Output file, will be in `.forest.car.zst` format.

          Will reuse the source name (with new extension) if pointed to a directory.

          [default: .]

      --compression-level <COMPRESSION_LEVEL>
          [default: 3]

      --frame-size <FRAME_SIZE>
          End zstd frames after they exceed this length

          [default: 8192]

      --force
          Overwrite output file without prompting

  -h, --help
          Print help (see a summary with '-h')
```

### `forest-tool snapshot compute-state`

```
Filecoin keeps track of "the state of the world", including: wallets and their balances; storage providers and their deals; etc...

It does this by (essentially) hashing the state of the world.

The world can change when new blocks are mined and transmitted. A block may contain a message to e.g transfer FIL between two parties. Blocks are ordered by "epoch", which can be thought of as a timestamp.

Snapshots contain (among other things) these messages.

The command calculates the state of the world at EPOCH-1, applies all the messages at EPOCH, and prints the resulting hash of the state of the world.

If --json is supplied, details about each message execution will printed.

Usage: forest-tool snapshot compute-state [OPTIONS] --epoch <EPOCH> <SNAPSHOT>

Arguments:
  <SNAPSHOT>
          Path to a snapshot CAR, which may be zstd compressed

Options:
      --epoch <EPOCH>
          Which epoch to compute the state transition for

      --json
          Generate JSON output

  -h, --help
          Print help (see a summary with '-h')
```

### `forest-tool fetch-params`

```
Download parameters for generating and verifying proofs for given size

Usage: forest-tool fetch-params [OPTIONS] [PARAMS_SIZE]

Arguments:
  [PARAMS_SIZE]  Size in bytes

Options:
  -a, --all              Download all proof parameters
  -k, --keys             Download only verification keys
  -d, --dry-run          Print out download location instead of downloading files
  -c, --config <CONFIG>  Optional TOML file containing forest daemon configuration
  -h, --help             Print help
```

### `forest-tool archive`

```
Manage archives

Usage: forest-tool archive <COMMAND>

Commands:
  info         Show basic information about an archive
  metadata     Show FRC-0108 metadata of an Filecoin snapshot archive
  f3-header    Show FRC-0108 header of a standalone F3 snapshot
  export       Trim a snapshot of the chain and write it to `<output_path>`
  checkpoints  Print block headers at 30 day interval for a snapshot file
  merge        Merge snapshot archives into a single file. The output snapshot refers to the heaviest tipset in the input set
  merge-f3     Merge a v1 Filecoin snapshot with an F3 snapshot into a v2 Filecoin snapshot in `.forest.car.zst` format
  diff         Show the difference between the canonical and computed state of a tipset
  sync-bucket  Export lite and diff snapshots from one or more CAR files, and upload them to an `S3` bucket
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-tool archive info`

```
Show basic information about an archive

Usage: forest-tool archive info <SNAPSHOT>

Arguments:
  <SNAPSHOT>  Path to an archive (`.car` or `.car.zst`)

Options:
  -h, --help  Print help
```

### `forest-tool archive export`

```
Trim a snapshot of the chain and write it to `<output_path>`

Usage: forest-tool archive export [OPTIONS] <SNAPSHOT_FILES>...

Arguments:
  <SNAPSHOT_FILES>...  Snapshot input path. Currently supports only `.car` file format

Options:
  -o, --output-path <OUTPUT_PATH>  Snapshot output filename or directory. Defaults to
                                   `./forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.zst`. [default: .]
  -e, --epoch <EPOCH>              Latest epoch that has to be exported for this snapshot, the upper bound. This value cannot be greater than the latest epoch available in the input snapshot
  -d, --depth <DEPTH>              How many state-roots to include. Lower limit is 900 for `calibnet` and `mainnet` [default: 2000]
      --diff <DIFF>                Do not include any values reachable from this epoch
      --diff-depth <DIFF_DEPTH>    How many state-roots to include when computing the diff set. All state-roots are included if this flag is not set
      --force                      Overwrite output file without prompting
  -h, --help                       Print help
```

### `forest-tool archive checkpoints`

```
Print block headers at 30 day interval for a snapshot file

Usage: forest-tool archive checkpoints <SNAPSHOT_FILES>...

Arguments:
  <SNAPSHOT_FILES>...  Path to snapshot file

Options:
  -h, --help  Print help
```

### `forest-tool archive f3-header`

```
Show FRC-0108 header of a standalone F3 snapshot

Usage: forest-tool archive f3-header <SNAPSHOT>

Arguments:
  <SNAPSHOT>  Path to a standalone F3 snapshot

Options:
  -h, --help  Print help
```

### `forest-tool archive metadata`

```
Show FRC-0108 metadata of an Filecoin snapshot archive

Usage: forest-tool archive metadata <SNAPSHOT>

Arguments:
  <SNAPSHOT>  Path to an archive (`.car` or `.car.zst`)

Options:
  -h, --help  Print help
```

### `forest-tool archive merge`

```
Merge snapshot archives into a single file. The output snapshot refers to the heaviest tipset in the input set

Usage: forest-tool archive merge [OPTIONS] <SNAPSHOT_FILES>...

Arguments:
  <SNAPSHOT_FILES>...  Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`

Options:
  -o, --output-path <OUTPUT_PATH>  Snapshot output filename or directory. Defaults to
                                   `./forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.zst`. [default: .]
      --force                      Overwrite output file without prompting
  -h, --help                       Print help
```

### `forest-tool archive merge-f3`

```
Merge a v1 Filecoin snapshot with an F3 snapshot into a v2 Filecoin snapshot in `.forest.car.zst` format

Usage: forest-tool archive merge-f3 --v1 <FILECOIN_V1> --f3 <F3> --output <OUTPUT>

Options:
      --v1 <FILECOIN_V1>  Path to the v1 Filecoin snapshot
      --f3 <F3>           Path to the F3 snapshot
      --output <OUTPUT>   Path to the snapshot output file in `.forest.car.zst` format
  -h, --help              Print help
```

### `forest-tool archive diff`

```
Show the difference between the canonical and computed state of a tipset

Usage: forest-tool archive diff [OPTIONS] --epoch <EPOCH> <SNAPSHOT_FILES>...

Arguments:
  <SNAPSHOT_FILES>...  Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`

Options:
      --epoch <EPOCH>  Selected epoch to validate
      --depth <DEPTH>
  -h, --help           Print help
```

### `forest-tool archive sync-bucket`

```
Export lite and diff snapshots from one or more CAR files, and upload them to an `S3` bucket

Usage: forest-tool archive sync-bucket [OPTIONS] <SNAPSHOT_FILES>...

Arguments:
  <SNAPSHOT_FILES>...


Options:
      --endpoint <ENDPOINT>
          `S3` endpoint URL

          [default: https://2238a825c5aca59233eab1f221f7aefb.r2.cloudflarestorage.com]

      --dry-run
          Don't generate or upload files, just show what would be done

      --export-mode <EXPORT_MODE>
          Export mode

          Possible values:
          - all:  Export all types of snapshots
          - lite: Export only lite snapshots
          - diff: Export only diff snapshots

          [default: all]

  -h, --help
          Print help (see a summary with '-h')
```

### `forest-tool db`

```
Database management

Usage: forest-tool db <COMMAND>

Commands:
  stats    Show DB stats
  destroy  DB destruction
  import   Import CAR files into the key-value store
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-tool db stats`

```
Show DB stats

Usage: forest-tool db stats [OPTIONS]

Options:
  -c, --config <CONFIG>  Optional TOML file containing forest daemon configuration
      --chain <CHAIN>    Optional chain, will override the chain section of configuration file if used
  -h, --help             Print help
```

### `forest-tool db destroy`

```
DB destruction

Usage: forest-tool db destroy [OPTIONS]

Options:
      --force            Answer yes to all forest-cli yes/no questions without prompting
  -c, --config <CONFIG>  Optional TOML file containing forest daemon configuration
      --chain <CHAIN>    Optional chain, will override the chain section of configuration file if used
  -h, --help             Print help
```

### `forest-tool db import`

```
Import CAR files into the key-value store

Usage: forest-tool db import [OPTIONS] --chain <CHAIN> <SNAPSHOT_FILES>...

Arguments:
  <SNAPSHOT_FILES>...  Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`

Options:
      --chain <CHAIN>    Filecoin network chain
      --db <DB>          Optional path to the database folder that powers a Forest node
      --skip-validation  Skip block validation
  -h, --help             Print help
```

### `forest-tool car`

```
Utilities for manipulating CAR files

Usage: forest-tool car <COMMAND>

Commands:
  concat    Concatenate two or more CAR files into a single archive
  validate  Check the validity of a CAR archive. For Filecoin-specific checks, see `forest-tool snapshot validate`
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-tool car concat`

```
Concatenate two or more CAR files into a single archive

Usage: forest-tool car concat --output <OUTPUT> [CAR_FILES]...

Arguments:
  [CAR_FILES]...  A list of CAR file paths. A CAR file can be a plain CAR, a zstd compressed CAR or a `.forest.car.zst` file

Options:
  -o, --output <OUTPUT>  The output `.forest.car.zst` file path
  -h, --help             Print help
```

### `forest-tool car validate`

```
Check the validity of a CAR archive. For Filecoin-specific checks, see `forest-tool snapshot validate`

Usage: forest-tool car validate [OPTIONS] <CAR_FILE>

Arguments:
  <CAR_FILE>  CAR archive. Supported extensions: `.car`, `.car.zst`, `.forest.car.zst`

Options:
      --ignore-block-validity  Skip verifying that blocks are hashed correctly
      --ignore-forest-index    Skip verifying the integrity of the on-disk index
  -h, --help                   Print help
```

### `forest-tool api`

```
API tooling

Usage: forest-tool api <COMMAND>

Commands:
  serve                   Starts an offline RPC server using provided snapshot files
  compare                 Compare two RPC providers
  generate-test-snapshot  Generates RPC test snapshots from test dump files and a Forest database
  dump-tests              Dumps RPC test cases for a specified API path
  test                    Runs RPC tests using provided test snapshot files
  test-stateful           Run multiple stateful JSON-RPC API tests against a Filecoin node
  help                    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-tool api serve`

```
Starts an offline RPC server using provided snapshot files.

This command launches a local RPC server for development and testing purposes. Additionally, it can be used to serve data from archival snapshots.

Usage: forest-tool api serve [OPTIONS] [SNAPSHOT_FILES]...

Arguments:
  [SNAPSHOT_FILES]...
          Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`

Options:
      --chain <CHAIN>
          Filecoin network chain

      --port <PORT>
          [default: 2345]

      --auto-download-snapshot


      --height <HEIGHT>
          Validate snapshot at given EPOCH, use a negative value -N to validate the last N EPOCH(s) starting at HEAD

          [default: -50]

      --index-backfill-epochs <INDEX_BACKFILL_EPOCHS>
          Backfill index for the given EPOCH(s)

          [default: 0]

      --genesis <GENESIS>
          Genesis file path, only applicable for devnet

      --save-token <SAVE_TOKEN>
          If provided, indicates the file to which to save the admin token

  -h, --help
          Print help (see a summary with '-h')
```

### `forest-tool api compare`

````
Compare two RPC providers.

The providers are labeled `forest` and `lotus`, but other nodes may be used (such as `venus`).

The `lotus` node is assumed to be correct and the `forest` node will be marked as incorrect if it deviates.

If snapshot files are provided, these files will be used to generate additional tests.

Example output: ```markdown | RPC Method                        | Forest              | Lotus         | |-----------------------------------|---------------------|---------------| | Filecoin.ChainGetBlock            | Valid               | Valid         | | Filecoin.ChainGetGenesis          | Valid               | Valid         | | Filecoin.ChainGetMessage (67)     | InternalServerError | Valid         | ``` The number after a method name indicates how many times an RPC call was tested.

Usage: forest-tool api compare [OPTIONS] [SNAPSHOT_FILES]...

Arguments:
  [SNAPSHOT_FILES]...
          Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`

Options:
      --forest <FOREST>
          Forest address

          [default: /ip4/127.0.0.1/tcp/2345/http]

      --lotus <LOTUS>
          Lotus address

          [default: /ip4/127.0.0.1/tcp/1234/http]

      --filter <FILTER>
          Filter which tests to run according to method name. Case sensitive

          [default: ]

      --filter-file <FILTER_FILE>
          Filter file which tests to run according to method name. Case sensitive. The file should contain one entry per line. Lines starting with `!` are considered as rejected methods, while the others are allowed. Empty lines and lines starting with `#` are ignored

      --filter-version <FILTER_VERSION>
          Filter methods for the specific API version

          Possible values:
          - v0: Only expose this method on `/rpc/v0`
          - v1: Only expose this method on `/rpc/v1`
          - v2: Only expose this method on `/rpc/v2`

      --fail-fast
          Cancel test run on the first failure

      --run-ignored <RUN_IGNORED>
          Behavior for tests marked as `ignored`

          [default: default]
          [possible values: default, ignored-only, all]

      --max-concurrent-requests <MAX_CONCURRENT_REQUESTS>
          Maximum number of concurrent requests

          [default: 8]

      --offline
          The nodes to test against is offline, the chain is out of sync

  -n, --n-tipsets <N_TIPSETS>
          The number of tipsets to use to generate test cases

          [default: 10]

      --miner-address <MINER_ADDRESS>
          Miner address to use for miner tests. Miner worker key must be in the key-store

      --worker-address <WORKER_ADDRESS>
          Worker address to use where key is applicable. Worker key must be in the key-store

      --eth-chain-id <ETH_CHAIN_ID>
          Ethereum chain ID. Default to the calibnet chain ID

          [default: 314159]

      --dump-dir <DUMP_DIR>
          Specify a directory to which the RPC tests are dumped

      --test-criteria-overrides [<TEST_CRITERIA_OVERRIDES>...]
          Additional overrides to modify success criteria for tests

          Possible values:
          - valid-and-timeout:   Test pass when first endpoint returns a valid result and the second one timeout
          - timeout-and-timeout: Test pass when both endpoints timeout

          [default: timeout-and-timeout]

      --report-dir <REPORT_DIR>
          Specify a directory to dump the test report

      --report-mode <REPORT_MODE>
          Report detail level: full (default), failure-only, or summary

          Possible values:
          - full:         Show everything
          - failure-only: Show summary and failures only
          - summary:      Show summary only

          [default: full]

  -h, --help
          Print help (see a summary with '-h')
````

### `forest-tool api generate-test-snapshot`

```
Generates RPC test snapshots from test dump files and a Forest database.

This command processes test dump files and creates RPC snapshots for use in automated testing. You can specify the database folder, network chain, and output directory. Optionally, you can allow generating snapshots even if Lotus and Forest responses differ, which is useful for non-deterministic tests.

See additional documentation in the <https://docs.forest.chainsafe.io/developers/guides/rpc_test_snapshot/>.

Usage: forest-tool api generate-test-snapshot [OPTIONS] --chain <CHAIN> --out-dir <OUT_DIR> <TEST_DUMP_FILES>...

Arguments:
  <TEST_DUMP_FILES>...
          Path to test dumps that are generated by `forest-tool api dump-tests` command

Options:
      --db <DB>
          Path to the database folder that powers a Forest node

      --chain <CHAIN>
          Filecoin network chain

      --out-dir <OUT_DIR>
          Folder into which test snapshots are dumped

      --use-response-from <USE_RESPONSE_FROM>
          Allow generating snapshot even if Lotus generated a different response. This is useful when the response is not deterministic or a failing test is expected. If generating a failing test, use `Lotus` as the argument to ensure the test passes only when the response from Forest is fixed and matches the response from Lotus

          [possible values: forest, lotus]

      --allow-failure
          Allow generating snapshot even if the test fails

  -h, --help
          Print help (see a summary with '-h')
```

### `forest-tool api dump-tests`

```
Dumps RPC test cases for a specified API path.

This command generates and outputs RPC test cases for a given API path, optionally including ignored tests. Useful for inspecting or exporting test cases for further analysis or manual review.

See additional documentation in the <https://docs.forest.chainsafe.io/developers/guides/rpc_test_snapshot/>.

Usage: forest-tool api dump-tests [OPTIONS] --path <PATH> [SNAPSHOT_FILES]...

Arguments:
  [SNAPSHOT_FILES]...
          Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`

Options:
      --offline
          The nodes to test against is offline, the chain is out of sync

  -n, --n-tipsets <N_TIPSETS>
          The number of tipsets to use to generate test cases

          [default: 10]

      --miner-address <MINER_ADDRESS>
          Miner address to use for miner tests. Miner worker key must be in the key-store

      --worker-address <WORKER_ADDRESS>
          Worker address to use where key is applicable. Worker key must be in the key-store

      --eth-chain-id <ETH_CHAIN_ID>
          Ethereum chain ID. Default to the calibnet chain ID

          [default: 314159]

      --path <PATH>
          Which API path to dump

          Possible values:
          - v0: Only expose this method on `/rpc/v0`
          - v1: Only expose this method on `/rpc/v1`
          - v2: Only expose this method on `/rpc/v2`

      --include-ignored


  -h, --help
          Print help (see a summary with '-h')
```

### `forest-tool api test`

```
Runs RPC tests using provided test snapshot files.

This command executes RPC tests based on previously generated test snapshots, reporting success or failure for each test. Useful for validating node behavior against expected responses.

See additional documentation in the <https://docs.forest.chainsafe.io/developers/guides/rpc_test_snapshot/>.

Usage: forest-tool api test <FILES>...

Arguments:
  <FILES>...
          Path to test snapshots that are generated by `forest-tool api generate-test-snapshot` command

Options:
  -h, --help
          Print help (see a summary with '-h')
```

### `forest-tool net ping`

```
Ping a peer via its `multiaddress`

Usage: forest-tool net ping [OPTIONS] <PEER>

Arguments:
  <PEER>  Peer `multiaddress`

Options:
  -c, --count <COUNT>        The number of times it should ping [default: 5]
  -i, --interval <INTERVAL>  The minimum seconds between pings [default: 1]
  -h, --help                 Print help
```

### `forest-tool shed`

```
Miscellaneous, semver-exempt commands for developer use

Usage: forest-tool shed <COMMAND>

Commands:
  summarize-tipsets          Enumerate the tipset CIDs for a span of epochs starting at `height` and working backwards
  peer-id-from-key-pair      Generate a `PeerId` from the given key-pair file
  private-key-from-key-pair  Generate a base64-encoded private key from the given key-pair file. This effectively transforms Forest's key-pair file into a Lotus-compatible private key
  key-pair-from-private-key  Generate a key-pair file from the given base64-encoded private key. This effectively transforms Lotus's private key into a Forest-compatible key-pair file. If `output` is not provided, the key-pair is printed to stdout as a base64-encoded string
  openrpc                    Dump the OpenRPC definition for the node
  migrate-state              Run a network upgrade migration
  help                       Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-tool shed summarize-tipsets`

```
Enumerate the tipset CIDs for a span of epochs starting at `height` and working backwards.

Useful for getting blocks to live test an RPC endpoint.

Usage: forest-tool shed summarize-tipsets [OPTIONS] --ancestors <ANCESTORS>

Options:
      --height <HEIGHT>
          If omitted, defaults to the HEAD of the node

      --ancestors <ANCESTORS>


  -h, --help
          Print help (see a summary with '-h')
```

### `forest-tool shed peer-id-from-key-pair`

```
Generate a `PeerId` from the given key-pair file

Usage: forest-tool shed peer-id-from-key-pair <KEYPAIR>

Arguments:
  <KEYPAIR>  Path to the key-pair file

Options:
  -h, --help  Print help
```

### `forest-tool shed private-key-from-key-pair`

```
Generate a base64-encoded private key from the given key-pair file. This effectively transforms Forest's key-pair file into a Lotus-compatible private key

Usage: forest-tool shed private-key-from-key-pair <KEYPAIR>

Arguments:
  <KEYPAIR>  Path to the key-pair file

Options:
  -h, --help  Print help
```

### `forest-tool shed openrpc`

```
Dump the OpenRPC definition for the node

Usage: forest-tool shed openrpc [OPTIONS] --path <PATH> [INCLUDE]...

Arguments:
  [INCLUDE]...


Options:
      --path <PATH>
          Which API path to dump

          Possible values:
          - v0: Only expose this method on `/rpc/v0`
          - v1: Only expose this method on `/rpc/v1`
          - v2: Only expose this method on `/rpc/v2`

      --omit <OMIT>
          A comma-separated list of fields to omit from the output (e.g., "summary,description")

          [possible values: summary, description]

  -h, --help
          Print help (see a summary with '-h')
```

### `forest-tool index`

```
Index database management

Usage: forest-tool index <COMMAND>

Commands:
  backfill  Backfill index with Ethereum mappings, events, etc
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-tool index backfill`

```
Backfill index with Ethereum mappings, events, etc

Usage: forest-tool index backfill [OPTIONS]

Options:
  -c, --config <CONFIG>        Optional TOML file containing forest daemon configuration
      --chain <CHAIN>          Optional chain, will override the chain section of configuration file if used
      --from <FROM>            The starting tipset epoch for back-filling (inclusive), defaults to chain head
      --to <TO>                Ending tipset epoch for back-filling (inclusive)
      --n-tipsets <N_TIPSETS>  Number of tipsets for back-filling
  -h, --help                   Print help
```

## `forest-dev`

```
forest-filecoin 0.31.1
ChainSafe Systems <info@chainsafe.io>
Rust Filecoin implementation.

USAGE:
  forest-dev <COMMAND>

SUBCOMMANDS:
  fetch-test-snapshots  Fetch test snapshots to the local cache
  state                 Interact with Filecoin chain state
  help                  Print this message or the help of the given subcommand(s)

OPTIONS:
  -h, --help     Print help
  -V, --version  Print version
```

### `forest-dev fetch-test-snapshots`

```
Fetch test snapshots to the local cache

Usage: forest-dev fetch-test-snapshots [OPTIONS]

Options:
      --actor-bundle <ACTOR_BUNDLE>
  -h, --help                         Print help
```

### `forest-dev state`

```
Interact with Filecoin chain state

Usage: forest-dev state <COMMAND>

Commands:
  compute          Compute state tree for an epoch
  replay-compute   Replay state computation with a db snapshot To be used in conjunction with `forest-dev state compute`
  validate         Validate tipset at a certain epoch
  replay-validate  Replay tipset validation with a db snapshot To be used in conjunction with `forest-dev state validate`
  help             Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `forest-dev state compute`

```
Compute state tree for an epoch

Usage: forest-dev state compute [OPTIONS] --epoch <EPOCH> --chain <CHAIN>

Options:
      --epoch <EPOCH>                Which epoch to compute the state transition for
      --chain <CHAIN>                Filecoin network chain
      --db <DB>                      Optional path to the database folder
      --export-db-to <EXPORT_DB_TO>  Optional path to the database snapshot `CAR` file to write to for reproducing the computation
  -h, --help                         Print help
```

### `forest-dev state replay-compute`

```
Replay state computation with a db snapshot To be used in conjunction with `forest-dev state compute`

Usage: forest-dev state replay-compute [OPTIONS] --chain <CHAIN> <SNAPSHOT>

Arguments:
  <SNAPSHOT>  Path to the database snapshot `CAR` file generated by `forest-dev state compute`

Options:
      --chain <CHAIN>  Filecoin network chain
  -n, --n <N>          Number of times to repeat the state computation [default: 1]
  -h, --help           Print help
```

### `forest-dev state validate`

```
Validate tipset at a certain epoch

Usage: forest-dev state validate [OPTIONS] --epoch <EPOCH> --chain <CHAIN>

Options:
      --epoch <EPOCH>                Tipset epoch to validate
      --chain <CHAIN>                Filecoin network chain
      --db <DB>                      Optional path to the database folder
      --export-db-to <EXPORT_DB_TO>  Optional path to the database snapshot `CAR` file to write to for reproducing the computation
  -h, --help                         Print help
```

### `forest-dev state replay-validate`

```
Replay tipset validation with a db snapshot To be used in conjunction with `forest-dev state validate`

Usage: forest-dev state replay-validate [OPTIONS] --chain <CHAIN> <SNAPSHOT>

Arguments:
  <SNAPSHOT>  Path to the database snapshot `CAR` file generated by `forest-dev state validate`

Options:
      --chain <CHAIN>  Filecoin network chain
  -n, --n <N>          Number of times to repeat the state computation [default: 1]
  -h, --help           Print help
```
