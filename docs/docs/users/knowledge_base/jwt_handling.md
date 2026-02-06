---
title: JWT Authentication
---

# JWT Authentication :key:

## What are JWTs?

JWTs (JSON Web Tokens) are the means Forest uses to authorize certain operations on the node. To read more about JWTs, see the [JWT.io website](https://jwt.io/introduction/).

## Lotus compatibility

The security model of JWTs in Forest is inspired and compatible with [Lotus](https://github.com/filecoin-project/lotus). This means that calls to the RPC API can be authorized using JWTs like in Lotus.

## How does Forest use JWTs?

During its initialization, the node generates a JWT private key. Then, the `admin` token is generated and printed at startup. Alternatively, use `--save-token <PATH>` to save the token to a file. This token grants full access to the node and its available methods. More fine-grained access can be granted by creating additional tokens, which will be explained later in this document.

:::info
The token is only printed once at startup. If you lose it, you can generate a new one by restarting the node.
:::

:::info
Technically, tokens have an expiration date, the default is 100 years, so there's no need to handle token expiration.
:::

:::danger
**Keep your tokens safe!** Anyone with access to the admin token can control your node if the RPC API is exposed to the internet. The private key is stored in an optionally encrypted file in the node's data directory. The default location on Linux is `$HOME/.local/share/forest/keystore` or `$HOME/.local/share/forest/keystore.json` if encryption is disabled. You should **not** disable keystore encryption in production environments.
:::

```shell
forest --chain calibnet --encrypt-keystore=false
```

Sample output:

```console
2024-08-21T11:26:37.608429Z  INFO forest::daemon::main: Using default calibnet config
2024-08-21T11:26:37.611063Z  INFO forest::daemon: Starting Forest daemon, version 0.19.2+git.76266421b1e
2024-08-21T11:26:37.611140Z  WARN forest::daemon: Forest has encryption disabled
### Admin token is printed here
2024-08-21T11:26:37.611185Z  INFO forest::daemon: Admin token: eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJBbGxvdyI6WyJyZWFkIiwid3JpdGUiLCJzaWduIiwiYWRtaW4iXSwiZXhwIjo0ODc3ODM5NTk3fQ.lnlboKjZhidbH177hWAD8m61MGwCu6w9AYCWaUZoepM
2024-08-21T11:26:37.611211Z  INFO forest::db::migration::db_migration: No database migration required
```

Alternative, with `--save-token <PATH>`:

```shell
forest --chain calibnet --encrypt-keystore=false --save-token /tmp/token --exit-after-init 2>&1 > /dev/null && cat /tmp/token
```

Sample output:

```console
eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJBbGxvdyI6WyJyZWFkIiwid3JpdGUiLCJzaWduIiwiYWRtaW4iXSwiZXhwIjo0ODc3ODM5NzM5fQ.Ra0u-js9GA0d7hHtJZ-7U4MGOMol5gkMveVeIQtgggw
```

## How to use JWTs

:::note
The admin token is assumed to be stored in `/tmp/token` for the following examples
:::

### via `forest-cli`

The most straightforward way to use tokens is to pass them to the `forest-cli` tool. This can be done either by passing it via the `--token` flag or by setting the `FULLNODE_API_INFO` environment variable. Note that the token is automatically set for CLI if it is invoked on the same host of the daemon.

```bash
forest-cli --token $(cat /tmp/token) shutdown
```

Format: `FULLNODE_API_INFO="<TOKEN>:/ip4/<host>/tcp/<port>/http`

```bash
FULLNODE_API_INFO="$(cat /tmp/token):/ip4/127.0.0.1/tcp/2345/http" forest-cli shutdown
```

### via HTTP headers

The token can be passed as a bearer token in the `Authorization` header when using the raw JSON-RPC API. Note the `Bearer` prefix, optional in Forest but required in Lotus.

```bash
curl --silent -X POST  \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $(cat /tmp/token)" \
    --data '{"jsonrpc":"2.0","id":2,"method":"Filecoin.Shutdown","param":"null"}' \
    "http://127.0.0.1:2345/rpc/v0"
{"jsonrpc":"2.0","id":2,"result":null}⏎
```

## Permissions

The admin token grants full access to the node. All exposed methods can be called, including `Filecoin.Shutdown`. There are different scopes for tokens:

- `admin` - _One token to rule them all, one token to find them, one token to bring them all, and in the darkness bind them._ - This token grants full access to the node and all its methods. It can also be used to create new tokens.
- `read` - read-only access. This grants access to methods that do not modify the state of the node, for example, `Filecoin.EthGasPrice`.
- `write` - `read` + `write` access. This grants, in addition to `read` access, access to methods that modify the state of the node, for example, `Filecoin.NetConnect`.
- `sign` - `read` + `write` + `sign` access. This grants, in addition to `read` access, access to methods that require signing, for example, `Filecoin.WalletSignMessage`.

## Creating new tokens

The tool for creating new tokens is `forest-cli auth create-token`. The `--perm` flag specifies the new token's permissions. They can be `admin`, `read`, `write`, or `sign`.

```bash
forest-cli --token $(cat /tmp/token) auth create-token --perm admin
```

Sample output:

```console
eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJBbGxvdyI6WyJyZWFkIiwid3JpdGUiLCJzaWduIiwiYWRtaW4iXSwiZXhwIjoxNzI5NTAzOTUzfQ.iRrbKNsujJsi89JauFPmFXM5DhgFc4hurtoncxN4pl8
```

Alternatively, you can use JSON-RPC method `Filecoin.AuthNew` to create new tokens, and `Filecoin.AuthVerify` to verify them.
