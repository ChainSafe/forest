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

## Message Pool

The Message Pool (mpool) is the component of forest that handles pending
messages that have reached the node for inclusion in the chain.

### Display the list of all pending messages

Usage: `forest-cli mpool pending`

Example:

```
{
  "Message": {
    "Version": 0,
    "To": "t01491",
    "From": "t3sg27lp6xgz3fb7db7t4x4lhmsf3dgu73mj5sodkshh64ftr6dzkrfxrowroon2cr2f3vkumsi4schkpfyvea",
    "Nonce": 14704,
    "Value": "0",
    "GasLimit": 31073678,
    "GasFeeCap": "100507",
    "GasPremium": "99453",
    "Method": 6,
    "Params": "iggZG3DYKlgpAAGC4gOB6AIgRRHJtEnHDx51h/M46ebVUjTD1kowbg+8uWOSrQnQYWwaAAaPXIAaAB45DvQAAAA=",
    "CID": {
      "/": "bafy2bzaceacz2f5k5pcjhzvodhpgin2phycgk2magezaxxp7wcqrjvobbtj5w"
    }
  },
  "Signature": {
    "Type": 2,
    "Data": "hcBY3OATkjMBRly96aViP2CR0R68dqnmlB1k6g2C2EXfe7+AsCN7bF4+M5bA6SecCsP2Fx+NwYkpGBi1CFGon5U9bqilMIiXxuK0mIrNO0d6UocCBGi/IVZwW2K4hT9N"
  },
  "CID": {
    "/": "bafy2bzaceacz2f5k5pcjhzvodhpgin2phycgk2magezaxxp7wcqrjvobbtj5w"
  }
}
```

### Display the CIDs of pending messages

Usage: `forest-cli mpool pending --cids`

### Display the locally published messages only

Usage: `forest-cli mpool pending --local`

### Display the list of all pending messages originating from a given address

Usage: `forest-cli mpool pending --from <address>`

### Display the list of all pending messages going to a given address

Usage: `forest-cli mpool pending --to <address>`

You can retrieve statistics about the current messages in the pool.

### Display statistics of all pending messages

Usage: `forest-cli mpool stat`

Example:

```
t3ub2uupkvfwp7zckda2songtluquirgxnooocjfifq6qesxre4igoc3u62njgvmmgnyccmowshbmrolkuni7a: Nonce past: 3, cur: 0, future: 1; FeeCap cur: 0, min-60: 0, gasLimit: 186447391
t3wikyuoalsqxathxey5jcsiowhbmy5o2ip6l4lvpna2rjxjd7micrgmlppjmwwcsnll7xgqzhlqqs6j4xk3oa: Nonce past: 1, cur: 0, future: 0; FeeCap cur: 0, min-60: 0, gasLimit: 66357410
t3wt6c4wla5egncjsgq67lsu4wzu4xtnbeskgupty7udysbiqkr4sw6inqli2nazks2ypwwnmlahtkzd4ghjja: Nonce past: 1, cur: 0, future: 0; FeeCap cur: 0, min-60: 0, gasLimit: 44752713
-----
total: Nonce past: 5, cur: 0, future: 1; FeeCap cur: 0, min-60: 0, gasLimit: 297557514
```

The `Nonce past`, `cur` (current) and `future` metrics indicate for each sending
account actor (the first address) how its message nonces are comparing
relatively to its own nonce.

A positive `past` number indicates messages that have been mined but that are
still present in the message pool and/or messages that could not get included in
a block. A positive `cur` number indicates all messages that are waiting to be
included in a block. A high number here could mean that the network is enduring
some congestion (if those messages are yours, you need to pay attention to the
different fees you are using and adjust them). A positive `future` number means
either that your forest node is not fully synced yet or if you are in sync that
some messages are using a too small nonce.

The `FeeCap cur` and `min-60` indicate how many messages from the sending
account actor have their basefee below to the current tipset basefee and the
minimum basefee found in the last 60 tipsets, respectively (use
`--basefee-lookback` flag to change the number of lookback tipsets).

The `gasLimit` value indicates the sum of `gasLimit` of all messages from each
sending actor.

The final `total` line is the accumulated sum of each metric for all messages.
