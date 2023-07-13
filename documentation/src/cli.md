# CLI

The Forest CLI allows for operations to interact with a Filecoin node and the
blockchain.

## Environment Variables

For nodes not running on a non-default port, or when interacting with a node
remotely, you will need to provide the multiaddress information for the node.
You will need to either set the environment variable `FULLNODE_API_INFO`, or
prepend it to the command, like so:

`FULLNODE_API_INFO="..." forest wallet new -s bls`

On Linux, you can set the environment variable with the following syntax

`export FULLNODE_API_INFO="..."`

Setting your API info this way will limit the value to your current session.
Look online for ways to persist this variable if desired.

The syntax for the `FULLNODE_API_INFO` variable is as follows:

`<admin_token>:/ip4/<ip of host>/tcp/<port>/http`

This will use IPv4, TCP, and HTTP when communicating with the RPC API. The admin
token can be found when starting the Forest daemon. This will be needed to
create tokens with certain permissions such as read, write, sign, or admin.

## Token flag

For nodes running on default port and when you are interacting locally, the
admin token can also be set using `--token` flag:

```
forest-cli --token eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJBbGxvdyI6WyJyZWFkIiwid3JpdGUiLCJzaWduIiwiYWRtaW4iXSwiZXhwIjoxNjczMjEwMTkzfQ.xxhmqtG9O3XNTIrOEB2_TWnVkq0JkqzRdw63BdosV0c <subcommand>
```

## Sending Filecoin tokens from your wallet

For sending Filecoin tokens, the Forest daemon must be running. You can do so by
running:

`forest --chain calibnet`

If your wallet is empty, you can fund your wallet using this
[faucet](https://faucet.calibration.fildev.network/funds.html). If this faucet
is unavailable or does not work, there is an
[alternative faucet](https://faucet.triangleplatform.com/filecoin/calibration).
You can verify your wallet was funded after a few minutes in
[Filscan](https://calibration.filscan.io/) by pasting the Message ID obtained
from the faucet. Example from
[this wallet](https://calibration.filscan.io/tipset/message-detail?cid=bafy2bzacebdverplts5qs3lwzsenzlh4rdsmvc42r6yg6suu4comr7gkbe76a).

Next, send Filecoin tokens to a wallet address:

`forest-cli --token <admin_token> send <wallet-address> <amount>`

where `<amount>` is an amount specified in any SI units; this field is
case-sensitive and can be specified in either short or long form (e.g., `mFIL`
or `milliFIL`) in the range from `quetta` to `quecto`. Note that the default
unit (if no unit is specified) is `FIL`.

## Wallet

Filecoin wallets are stored under the Forest data directory (e.g.,
`~/.local/share/forest` in the case of Linux) in a `keystore` file.

All wallet commands require write permissions and an admin token (`--token`) to
interact with the keystore. The admin token can be retrieved from forest startup
logs or by including the flag `--save-token <PATH>` during `forest` invocation.

### Balance:

Retrieve the FIL balance of a given address. Usage:
`forest-cli --token <admin_token> wallet balance <address>`

### Default:

Get the default, persisted address from the keystore. Usage:
`forest-cli --token <admin_token> wallet default`

### Has:

Check if an address exists in the keystore. Outputs `true` if address exists and
`false` if address does not exist. Usage:
`forest-cli --token <admin_token> wallet has <address>`

### List:

Display the keys in the keystore. Usage:
`forest-cli --token <admin_token> wallet list`

### New:

Create a new wallet. The signature type can either be secp256k1 or bls. Defaults
to use secp256k1. Usage:
`forest-cli --token <admin_token> wallet new [ bls | secp256k1 ]`

### Set-default:

Set an address to be the default address of the keystore. Usage:
`forest-cli --token <admin_token> wallet set-default <address>`

### Export:

Export a key by address. Use a wallet address to export a key. Returns a
formatted key to be used to import on another node, or into a new keystore.
Usage: `forest-cli --token <admin_token> wallet export <address>`

### Import:

Import a file containing a private key to the keystore and create a new address.
The default format for importing keys is hex encoded JSON. Use the `export`
command to get formatted keys for importing. Usage:
`forest-cli --token <admin_token> wallet import <PATH>`

### Sign:

Use an address to sign a vector of bytes. Usage:
`forest-cli --token <admin_token> wallet sign -m <hex message> -a <address>`

### Verify:

Verify the message's integrity with an address and signature. Outputs `true` if
signature verifies message integrity, otherwise `false`. Usage:
`forest-cli wallet verify -m <hex message> -a <address> -s <signature>`

## Chain-Sync

The chain-sync CLI can mark blocks to never be synced, provide information about
the state of the syncing process, and check blocks that will never be synced
(and for what reason).

Wait Wait for the sync process to be complete Usage: `forest-cli sync wait`
Permissions: Read

Status Check the current state of the syncing process, displaying some
information Usage: `forest-cli sync status` Permissions: Read

Check Bad Check if a block has been marked by, identifying the block by CID
Usage: `forest-cli sync check-bad -c <block cid>` Permissions: Read

Mark Bad Mark a block as bad, the syncer will never sync this block Usage:
`forest-cli sync mark-bad -c <block cid>` Permissions: Admin
