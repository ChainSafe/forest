---
title: Interacting With Wallets
sidebar_position: 3
---

# Wallets in Forest

The Forest client provides two types of wallets:

1. **Local wallet (only accessible by you)**: This wallet is recommended for day-to-day use due to its higher security. Since it is only accessible by you, it minimizes exposure and reduces the likelihood of compromise.

2. **Node wallet (accessible by the Forest node)**: This wallet is managed by the Forest node and is included for backward compatibility with Lotus. It’s less secure as the node may have direct access to it for network operations. This could potentially expose it to unauthorized access or other network-related vulnerabilities.

In the following sections, we will be using the wallet in its local mode.

## Configuration

To query an account's balance or transfer funds, you need access to a running Filecoin node. You can run such a node yourself or use a publicly available node.

[Glif.io](https://www.glif.io/en) runs a public Filecoin node that we can use by setting the `FULLNODE_API_INFO` environment variable:

```shell
export FULLNODE_API_INFO=/dns/api.calibration.node.glif.io/tcp/443/https
```

:::caution

As a rule of thumb, only send mainnet FIL tokens through a node that you trust.
The rest of this document will assume you're using testnet tokens.

:::

:::note

The `forest-wallet` will figure out the network you are using based on the running node. No additional configuration is required.

:::

## Creating an account

Initially, our wallet contains no addresses:

```shell
forest-wallet list
```

Should output:

```console
Address                                   Default Balance
```

Let's create a new address and inspects its balance:

```shell
forest-wallet new
```

Sample output:

```console
t1amfhh3hxvsilyhloxwheuxforst5hyzsbletgoy
```

Listing the accounts shows the new account with a balance of `0 FIL`:

```shell
forest-wallet list
```

Sample output:

```console
Address                                   Default Balance
t1amfhh3hxvsilyhloxwheuxforst5hyzsbletgoy  X        0 FIL
```

You can make sure you are using testnet addresses by checking their prefix. They start with a `t` whereas mainnet ones start with a `f`.

:::note

You can read more about the different address types [here](https://docs.filecoin.io/smart-contracts/filecoin-evm-runtime/address-types).

:::

The generated address will be unique and it will have a balance of `0 FIL`.
Since this is a testnet account, we can add FIL to it from the [faucet](https://faucet.calibnet.chainsafe-fil.io/funds.html)/[alternate faucet](https://faucet.triangleplatform.com/filecoin/calibration).

After requesting the funds and waiting roughly a minute, we can see the funds arrive in our wallet:

```shell
forest-wallet list
```

Sample output:

```console
Address                                   Default Balance
t1amfhh3hxvsilyhloxwheuxforst5hyzsbletgoy  X        100 FIL
```

## Sending FIL tokens from your wallet

Let's create a new, empty account:

```shell
forest-wallet new
```

Sample output:

```console
t1qj55ggurqydu4mgoon7ycvkyyhofc4tvf25tmlq
```

Listing the accounts shows the new account with a balance of `0 FIL`:

```shell
forest-wallet list
```

Sample output:

```console
Address                                   Default Balance
t1qj55ggurqydu4mgoon7ycvkyyhofc4tvf25tmlq           0 FIL
t1amfhh3hxvsilyhloxwheuxforst5hyzsbletgoy  X        100 FIL
```

We can transfer FIL to this new account from our default account:

```shell
forest-wallet send t1qj55ggurqydu4mgoon7ycvkyyhofc4tvf25tmlq "1.2 FIL"
```

Sample output:

```console
bafy2bzaceblzz644szs6s5ggyxlgdnonlq5bavu54cxwujcdtgdaze2bafdle
```

It takes a minute or so for the message to be included in the Filecoin blockchain. Once the message has gone through, we can inspect our balances again:

```shell
forest-wallet list
```

Sample output:

```console
Address                                   Default Balance
t1qj55ggurqydu4mgoon7ycvkyyhofc4tvf25tmlq           1200 milliFIL
t1amfhh3hxvsilyhloxwheuxforst5hyzsbletgoy  X        ~98800 milliFIL
```

:::tip

When requesting funds from the faucet or sending tokens to another address, the CID of the message will be shown. You can use it to inspect the message on any [Filecoin Blockchain Explorer](https://docs.filecoin.io/networks/calibration/explorers).

:::

The gas cost of the message is automatically paid from the sending account.

## Lotus compatibility

If you want to use the builtin wallet in a Lotus or Forest node, you can use the `forest-wallet` executable with the `--remote-wallet` option. The subcommands remain the same but require write access to the remote Filecoin node.

## Security recommendations

To maximize the security of your local wallet, we recommend following these best practices:

- **Set strict file permissions**: Ensure that the local wallet file can only be accessed by your user account by running the following command:

  ```shell
  chmod 600 ~/.local/share/forest-wallet/keystore.json
  ```

  This makes the file readable and writable only by you. Note that this is done automatically when you create a new wallet, but it is good to check if you have modified the permissions later.

- **Encrypt your wallet data**: Encryption provides an additional layer of security in case your system is compromised or accessed by others. You can either encrypt your entire disk or, at a minimum, your home directory. At the very least, encrypt the keystore with a strong password. You can pass the `--encrypt` flag when creating a new wallet:

  ```shell
  forest-wallet --encrypt new
  ```

  From now on, you will be prompted for the password whenever you access the wallet.

- **Create backups**: Regularly back up your local wallet. This ensures that if something happens to your system, you will still have access to your funds.
  Please refer to the `forest-tool backup` subcommands.

:::danger

**Never share your wallet with anyone**: Your wallet contains your private key, which gives full control over your funds. No legitimate service will ever ask you to share your private key or wallet file.

:::
