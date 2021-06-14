
# CLI

The Forest CLI allows for operations to interact with a Filecoin node and the blockchain.


## Environment Variables
For nodes not running on a non-default port, or when interacting with a node remotely, you will need
to provide the multiaddress information for the node. You will need to either set the environment variable
`FULLNODE_API_INFO`, or prepend it to the command, like so:

`FULLNODE_API_INFO="..." forest wallet new -s bls`

On Linux, you can set the environment variable with the following syntax

`export FULLNOPDE_API_INFO="..."`

Setting your API info this way will limit the value to your current session. Look online for ways to persist
this variable if desired.

The syntax for the `FULLNODE_API_INFO` variable is as follows:

`<admin_token>:/ip4/<ip of host>/tcp/<port>/http`

This will use IPv4, TCP, and HTTP when communicating with the RPC API. The admin token can be found when starting
the Forest daemon. This will be needed to create tokens with certain permissions such as read, write, sign, or admin.

## Wallet

All wallet commands require write permissions to interact with the keystore

Balance
Retrieve the FIL balance of a given address
Usage: `forest wallet balance <address>`

Default
Get the default, persisted address from the keystore
Usage: `forest wallet default`

Has
Check if an address exists in the keystore
shows true/false if exists or doesn't
Usage: `forest wallet has <address>`

List
Display the keys in the keystore
Usage: `forest wallet list`

New
Create a new wallet
The signature type can either be secp256k1 or bls. Defaults to use bls
Usage: `forest wallet new <bls/secp256k1>`

Set-default
Set an address to be the default address of the keystore
Usage: `forest wallet set-default <address>`

Import
Import a private key to the keystore and create a new address.
The default format for importing keys is hex encoded JSON. Use the `export`
command to get formatted keys for importing.
Usage: `forest wallet import <hex encoded json key>`

Export
Export a key by address. Use a wallet address to export a key. Returns a formatted key
to be used to import on another node, or into a new keystore.
Usage: `forest wallet export <address>`

Sign
Use an address to sign a vector of bytes
Usage: `forest wallet sign -m <hex message> -a <address>`

Verify
Verify the message's integrity with an address and signature
Usage: `forest wallet verify -m <hex message> -a <address> -s <signature>`
