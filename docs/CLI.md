
# CLI

The forest CLI allows for operations to interact with a Filecoin node and the blockchain.


## Environment Variables
For nodes not running on a non-default port, or when interacting with a node remotely, you will need
to provide the multiaddress information for the node. You will need to either set the environment variable
`FULLNODE_API_INFO`, or prepend it to the command, like so:

`FULLNODE_API_INFO="..." forest wallet new -s bls`

On linux, you can set the environment variable with the following syntax

`export FULLNOPDE_API_INFO="..."`

Setting your API info this way will limit the value to your current session. Look online for ways to persist 
this variable if desired.

The syntax for the `FULLNODE_API_INFO` variable is as follows:

`<admin_token>:/ip4/<ip of host>/tcp/<port>/http`

This will use IPv4, tcp, and http when communicating with the RPC API. The admin token can be found when starting
the forest daemon. This will be needed to create tokens with certain permissions such as read, write, sign, or admin.

## Wallet

Balance
Retrieve the fil balance of a given address
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
