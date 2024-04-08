# Wallet handling

There are two wallets for Forest: One accessible by the Forest node, and one
that is only accessible by you. It's recommended that you only use the local
wallet for security reasons. The wallet in the Forest node exists for backward
compatiblity with Lotus.

## Configuration

To query the balance of an account or to transfer funds, you need access to a
running Filecoin node. You can either run such a node yourself or use a publicly
available node. As a rule of thumb, don't send real money through a node that
you do not trust. The rest of this document will assume you're using play money
on calibnet.

Glif.io runs a public Filecoin node at that we can use by setting
`FULLNODE_API_INFO`:

```bash
export FULLNODE_API_INFO=/dns/api.calibration.node.glif.io/tcp/443/https
```

## Creating an account

Initially, our wallet contains no addresses:

```
$ forest-wallet list
Address                                   Default Balance
```

Let's create a new address and inspects its balance:

```
$ forest-wallet new
t15ydyu3d65gznpp2qxwpkjsgz4waubeunn6upvla
$ forest-wallet list
Address                                   Default Balance
t15ydyu3d65gznpp2qxwpkjsgz4waubeunn6upvla  X        0 FIL
```

The generated address will be unique and it will have a balance of `0 FIL`.
Since this is a testnet account, we can add FIL to it from the
[faucet](https://faucet.calibnet.chainsafe-fil.io/funds.html)/[alternate faucet](https://faucet.triangleplatform.com/filecoin/calibration).

After requesting the funds and waiting roughly a minute, we can see the funds
arrive in our wallet:

```
$ forest-wallet list
Address                                   Default Balance
t15ydyu3d65gznpp2qxwpkjsgz4waubeunn6upvla  X        100 FIL
```

## Sending Filecoin tokens from your wallet

Let's create a new, empty account:

```
$ forest-wallet new
t14tgmcxrcohfstxuxfbfk2vrjr3tqmefzlajp52y
$ forest-wallet list
Address                                   Default Balance
t14tgmcxrcohfstxuxfbfk2vrjr3tqmefzlajp52y           0 FIL
t15ydyu3d65gznpp2qxwpkjsgz4waubeunn6upvla  X        100 FIL
```

We can transfer FIL to this new account from our default account:

```
$ forest-wallet send t14tgmcxrcohfstxuxfbfk2vrjr3tqmefzlajp52y "1.2 FIL"
bafy2bzaceasy7bzgjwnl4mbjp3tfxdeq4mvdvfne7fj773w7x4d6ah7cdabkc
```

It takes a minute or so for the transaction to be included in the Filecoin
blockchain. Once the transaction has gone through, we can inspect our balances:

```
$ forest-wallet list
Address                                   Default Balance
t14tgmcxrcohfstxuxfbfk2vrjr3tqmefzlajp52y           1200 milliFIL
t15ydyu3d65gznpp2qxwpkjsgz4waubeunn6upvla  X        ~98800 milliFIL
```

The gas cost of the transaction is automatically paid from the sending account.

## CLI

The forest-wallet executable offers several subcommand and options:

```
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
  validate-address  Validates whether a given string can be decoded
                    as a well-formed address
  verify            Verify the signature of a message. Returns true
                    if the signature matches the message and address
  delete            Deletes the wallet associated with the given address
  send              Send funds between accounts
  help              Print this message or the help of the given subcommand(s)

OPTIONS:
      --token <TOKEN>  Admin token to interact with the node
      --remote-wallet  Use remote wallet associated with the Filecoin node
      --encrypt        Encrypt local wallet
  -h, --help           Print help
  -V, --version        Print version
```

## Lotus compatiblity

If you want to use the builtin wallet in a Lotus or Forest node, you can use the
`forest-wallet` executable with the `--remote-wallet` option. The subcommands
remain the same but they require write access to the remote Filecoin node.
