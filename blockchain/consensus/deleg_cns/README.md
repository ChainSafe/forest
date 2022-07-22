# Delegated Consensus

_Delegated Consensus_ is a simplistic consensus protocol made for demo purposes, where only a single miner is allowed to propose blocks - this miner is who the others "delegate to".

## Setup Genesis

To get started, we need to generate a `genesis.car` file that contains the public key of this miner, so that participants can validate that the blocks they receive are signed by the expected miner.

At the time of this writing _Forest_ doesn't have the necessary machinery to prepare its own `genesis.car` file (although it has [commands](https://github.com/ChainSafe/forest/blob/3d149dcd7cfd23a8ce2793e107a4c7db20948584/forest/src/cli/genesis_cmd.rs#L19) to prepare a template for it), so for the time being we have to rely on [Lotus CLI](https://github.com/filecoin-project/lotus/tree/v1.17.0-rc3/cmd) to do this for us.


### Build Lotus

Ideally we could avoid having to build Lotus just for this purpose, and use the published [Docker image](https://hub.docker.com/r/filecoin/lotus). However, not all commands that we need are part of the image; for example `lotus-seed` is only available in the [lotus-test](https://github.com/filecoin-project/lotus/blob/v1.17.0-rc3/Dockerfile.lotus#L232) build target.

Still, we can use docker to build the CLI without having to install all necessary build dependencies ourselves. Assuming that we already have [Lotus](https://github.com/filecoin-project/lotus) cloned from Github and we are in the top level `lotus` directory, we can use the following command to build an image we need:

```bash
docker build -t filecoin/lotus-test -f Dockerfile.lotus --target lotus-test .
```

### Generate signing keys

According to the [spec](https://spec.filecoin.io/#section-systems.filecoin_nodes.repository.key_store) we need to generate a BLS key for signing blocks, a.k.a. the "worker" key.

We could use the [lotus-keygen](https://github.com/filecoin-project/lotus/tree/v1.17.0-rc3/cmd/lotus-keygen) command to generate Secp256k1 or BLS keys, but unfortunately this command is not copied into the Docker image we built. However, that's not a problem in this case becase the `lotus-seed` command will generate a key for us, if it's not given an existing one.

### Generate the pre-seal file

Lotus will generate quite a few different files, so let's create a directory to put them into.

```bash
mkdir -p genesis-files
```

The first file we generate will be the `preseal.json`, which contains the worker keys to that we want to put in the `genesis.json` file later.

```bash
docker run -it --rm \
  -v $PWD/genesis-files:/out \
  --user $(id -u):$(id -g) \
  --entrypoint lotus-seed \
  filecoin/lotus-test \
    --sector-dir /out \
    pre-seal --miner-addr t01000
```

Note that we used `t01000` as the miner address, which is also the default, and practically the only one we can use (besides its mainnet variant) as the ID of the first miner, because Lotus insists the numbering starts from `1000`.

Let's see what we have:

```console
$ ls genesis-files
cache  pre-seal-t01000.json  pre-seal-t01000.key  sealed  sectorstore.json  unsealed  update  update-cache
```

The [pre-seal command](https://github.com/filecoin-project/lotus/blob/v1.17.0-rc3/cmd/lotus-seed/main.go#L59) generated two a `pre-seal-t01000.json` file that we have to merge into a Genesis template, and also a `pre-seal-t01000.key` file, which contains a [generated BLS key](https://github.com/filecoin-project/lotus/blob/9794652e0be9a7709934580f36a4eeaa894ea2ad/cmd/lotus-seed/seed/seed.go#L93) that we will use to sign blocks later.

We can ignore the rest of the files because we will not be doing any actual Filecoin mining.

### Generate a Genesis template

The next command we need is [genesis new](https://github.com/filecoin-project/lotus/blob/v1.17.0-rc3/cmd/lotus-seed/genesis.go#L48) to get ourselves a `genesis.json` file.


```bash
docker run -it --rm \
  -v $PWD/genesis-files:/out \
  --user $(id -u):$(id -g) \
  --entrypoint lotus-seed \
  filecoin/lotus-test \
    --sector-dir /out \
    genesis new "/out/genesis.json"
```

### Add Miner

Next we have to add the the miner we created earlier to the Genesis template using the [genesis add-miner](https://github.com/filecoin-project/lotus/blob/v1.17.0-rc3/cmd/lotus-seed/genesis.go#L90) command.

```bash
docker run -it --rm \
  -v $PWD/genesis-files:/out \
  --user $(id -u):$(id -g) \
  --entrypoint lotus-seed \
  filecoin/lotus-test \
    --sector-dir /out \
    genesis add-miner "/out/genesis.json" "/out/pre-seal-t01000.json"
```

The command modifies `genesis.json` in place and appends to the `Accounts` and `Miners` collections, and also adds an initial balance of 50,000,000 to the miner Actor.

Let's see what we have in the end:

```console
$ cat genesis-files/genesis.json
{
  "NetworkVersion": 16,
  "Accounts": [
    {
      "Type": "account",
      "Balance": "50000000000000000000000000",
      "Meta": {
        "Owner": "t3xe65dytq7cislbtg3f5clk47bl3mmbl6kurugybsfjzzn7l2tws5fhpvcttcmz4j3zedur7a6zc3snwk67pq"
      }
    }
  ],
  "Miners": [
    {
      "ID": "t01000",
      "Owner": "t3xe65dytq7cislbtg3f5clk47bl3mmbl6kurugybsfjzzn7l2tws5fhpvcttcmz4j3zedur7a6zc3snwk67pq",
      "Worker": "t3xe65dytq7cislbtg3f5clk47bl3mmbl6kurugybsfjzzn7l2tws5fhpvcttcmz4j3zedur7a6zc3snwk67pq",
      "PeerId": "12D3KooWLAL8FXK5tX2H7k1jqDWngs5D12Sbpaqq1ukQuk8HNGbY",
      "MarketBalance": "0",
      "PowerBalance": "0",
      "SectorSize": 2048,
      "Sectors": [
        {
          "CommR": {
            "/": "bagboea4b5abcbo7z76pf75mq6zt3cbx7n4isafgsg6nzldi73mm5utarwdmt3zls"
          },
          "CommD": {
            "/": "baga6ea4seaqctekgwa4cesh46ftxapju4swvbkacu6rwlorvih6l4eucji34qhq"
          },
          "SectorID": 0,
          "Deal": {
            "PieceCID": {
              "/": "baga6ea4seaqctekgwa4cesh46ftxapju4swvbkacu6rwlorvih6l4eucji34qhq"
            },
            "PieceSize": 2048,
            "VerifiedDeal": false,
            "Client": "t3xe65dytq7cislbtg3f5clk47bl3mmbl6kurugybsfjzzn7l2tws5fhpvcttcmz4j3zedur7a6zc3snwk67pq",
            "Provider": "t01000",
            "Label": "0",
            "StartEpoch": 0,
            "EndEpoch": 9001,
            "StoragePricePerEpoch": "0",
            "ProviderCollateral": "0",
            "ClientCollateral": "0"
          },
          "DealClientKey": {
            "Type": "bls",
            "PrivateKey": "2ooyr9iChbMChrUK+en94RYKr1FlEj1Oa9PglTem1iM=",
            "PublicKey": "uT3R4nD4kSWGZtl6JaufCvbGBX5VI0NgMipzlv16naXSnfUU5iZnid5IOkfg9kW5",
            "Address": "t3xe65dytq7cislbtg3f5clk47bl3mmbl6kurugybsfjzzn7l2tws5fhpvcttcmz4j3zedur7a6zc3snwk67pq"
          },
          "ProofType": 5
        }
      ]
    }
  ],
  "NetworkName": "localnet-850f257a-5cb0-4952-8cad-beee1c462061",
  "VerifregRootKey": {
    "Type": "multisig",
    "Balance": "0",
    "Meta": {
      "Signers": [
        "t1ceb34gnsc6qk5dt6n7xg6ycwzasjhbxm3iylkiy"
      ],
      "Threshold": 1,
      "VestingDuration": 0,
      "VestingStart": 0
    }
  },
  "RemainderAccount": {
    "Type": "multisig",
    "Balance": "0",
    "Meta": {
      "Signers": [
        "t1ceb34gnsc6qk5dt6n7xg6ycwzasjhbxm3iylkiy"
      ],
      "Threshold": 1,
      "VestingDuration": 0,
      "VestingStart": 0
    }
  }
}
```

### Generate a CAR file

All that's left is for the `genesis.json` file to be turned into `genesis.car` by running the [genesis car](https://github.com/filecoin-project/lotus/blob/v1.17.0-rc3/cmd/lotus-seed/genesis.go#L559) command, which will create the Genesis block and the initial State Tree, and save it in the IPLD format that Forest expects to be started with.

```bash
docker run -it --rm \
  -v $PWD/genesis-files:/out \
  --user $(id -u):$(id -g) \
  --entrypoint lotus-seed \
  filecoin/lotus-test \
    --sector-dir /out \
    genesis car --out "/out/genesis.car" "/out/genesis.json"
```

The log helpfully tells us what is happening:

```console
2022-07-19T13:38:55.057Z	WARN	genesis	testing/genesis.go:57	Generating new random genesis block, note that this SHOULD NOT happen unless you are setting up new network
init set t3w3sv7uykya4vkzk77d2h7lfuldtaa5rr3nvzh6fkutu5lesdwefgnqm2u3obpyrhhzbupl42kubzqmvfivjq t0100
init set t1ceb34gnsc6qk5dt6n7xg6ycwzasjhbxm3iylkiy t0101
[flexi_logger][ERRCODE::Time] flexi_logger has to work with UTC rather than with local time, caused by IndeterminateOffset
    See https://docs.rs/flexi_logger/latest/flexi_logger/error_info/index.html#time
publishing 1 storage deals on miner t01000 with worker t3w3sv7uykya4vkzk77d2h7lfuldtaa5rr3nvzh6fkutu5lesdwefgnqm2u3obpyrhhzbupl42kubzqmvfivjq
0.000001621003269618 FIL
2022-07-19T13:38:58.463Z	INFO	genesis	genesis/genesis.go:591	Empty Genesis root: bafy2bzacedswlcz5ddgqnyo3sak3jmhmkxashisnlpq6ujgyhe4mlobzpnhs6
GENESIS MINER ADDRESS: t01000
2022-07-19T13:38:58.468Z	WARN	genesis	testing/genesis.go:97	WRITING GENESIS FILE AT /out/genesis.car
```

and we have our Genesis file:

```console
$ ls -lh genesis-files/*.car
-rw-r--r-- 1 aakoshh aakoshh 5.3M Jul 19 14:38 genesis-files/genesis.car
```

If we look carefully at the log we can see that it says `init set t3w3...ivjq t0100`, which is _not_ the `t01000` ID we might have expected. This is because a miner has two Actors representing it in the system: one with a `miner::State` and another with an `account::State`; the latter is identified by `t0100`, which is the first ID issued by the system.

For now we work within the common validation framework of Forest, which expects to see a miner ID in the header, not the account ID, but this works differently in Eudico, where they use the account ID directly. Both can be resolved to a public key, to check the block signature.

### Clean Up

The `genesis.car` file will need to be present on each node we add to the network. The other one we need to keep is `pre-seal-t01000.key` but _only_ for the process that will do the block production. It's probably a good idea to keep `genesis.json` as well in case we want to remind ourselves about the public key. We don't strictly need it because we will use the miner ID (`t01000`) as a configuration value to each participant to let them know about which miner to expect to sign the blocks - the corresponding key will be looked up in the ledger.

The rest of the files can be deleted.

```console
$ cd genesis-files && ls | grep -v pre-seal | grep -v genesis | xargs rm -rf && cd -
$ ls genesis-files
genesis.car  genesis.json  pre-seal-t01000.json  pre-seal-t01000.key
```

Optionally the Docker image can be removed as well:

```bash
docker rmi filecoin/lotus-test
docker system prune -f
```

### All together now

The [scripts/generate-genesis-files.sh](scripts/generate-genesis-files.sh) file is a convenience script that runs all the above commands assuming that `lotus-seed` is on the `PATH`, which may be the case if we already have Lotus or Eudico installed. If so, we can run the script directly, giving it the output directory (which must exist) to write the genesis files to.

And similarly we can use the docker image we prepared here to run all the above commands as a single step:

```bash
docker run -it --rm \
  -v $PWD/genesis-files:/out \
  -v $PWD/scripts:/scripts \
  --user $(id -u):$(id -g) \
  --entrypoint /scripts/generate-genesis-files.sh \
  filecoin/lotus-test /out
```

## Add the private key to a wallet

One of the Forest processes we start will need access to the private key, and normally it looks for it in the wallet. To prepare a wallet, we can use Forest itself because it seems to have all the necessary [commands](https://github.com/ChainSafe/forest/blob/v0.3.0/forest/src/cli/wallet_cmd.rs).

First of all, let's build the binary.

```bash
make build
```

It's also possible to run `make install` to make it available globally, but for this exercise we can just run it from the build artifacts; make can make an alias so the commands look identical.

```bash
alias forest=target/debug/forest
```

The wallet commands need a running node to connect to, so first we need to start a Forest deamon.

```bash
./target/debug/forest --chain calibnet --import-snapshot ../calibnet-2022-07-01.car --target-peer-count 50 --encrypt-keystore false
```

The snapshot was downloaded from [here](https://fra1.digitaloceanspaces.com/forest-snapshots/calibnet/calibnet-2022-07-01.car?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=3JPB6WZESYS26MFHEJW5%2F20220714%2Ffra1%2Fs3%2Faws4_request&X-Amz-Date=20220714T155556Z&X-Amz-Expires=604800&X-Amz-SignedHeaders=host&X-Amz-Signature=e554ffe1b5c810fff9125e29647a538bfeda0bb5fcdc7f09d1511f868c0a7ed4). Without it the daemon won't be able to sync with the tesnet because it doesn't support earlier network versions or migrations. It also won't start the JSON-RPC interface until it has imported the snapshot, so we have to wait about ~30 minutes for the node to be ready for us to send commands.

```console
$ ./target/debug/forest --chain calibnet --import-snapshot ../calibnet-2022-07-01.car --target-peer-count 5 --encrypt-keystore false
 2022-07-22T12:21:27.835Z WARN  forest::cli > No configurations found, using defaults.
 2022-07-22T12:21:27.836Z INFO  forest::daemon > Starting Forest daemon, version v0.2.2/unstable/a121904e
 2022-07-22T12:21:27.836Z INFO  forest_libp2p::service > Networking keystore not found!
 2022-07-22T12:21:27.837Z INFO  utils                  > Permissions set to 0600 on File { fd: 6, path: "/home/aakoshh/.local/share/forest/libp2p/keypair", read: false, write: true }
 2022-07-22T12:21:27.838Z WARN  forest::daemon         > Warning: Keystore encryption disabled!
 2022-07-22T12:21:27.838Z WARN  key_management::keystore > Keystore does not exist, initializing new keystore at: "/home/aakoshh/.local/share/forest/keystore.json"
 2022-07-22T12:21:27.838Z INFO  utils                    > Permissions set to 0600 on File { fd: 6, path: "/home/aakoshh/.local/share/forest/keystore.json", read: false, write: true }
 2022-07-22T12:21:27.839Z INFO  metrics                  > Prometheus server started at 0.0.0.0:6116
 2022-07-22T12:21:27.839Z INFO  forest::daemon           > Admin token: eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJBbGxvdyI6WyJyZWFkIiwid3JpdGUiLCJzaWduIiwiYWRtaW4iXX0.1zvrh2cLDyJl9K3fXeURy4sLZyEQPwHa7-p26VA_ztg
 2022-07-22T12:21:27.906Z WARN  chain::store::chain_store > No previous chain state found
 2022-07-22T12:21:27.946Z INFO  genesis                   > Initialized genesis: BlockHeader: Cid(bafy2bzacecz3trtejxtzix4f4eebs7dekm6snfsmvffiqz2rfx7iwgsgtieq4)
 2022-07-22T12:21:28.002Z INFO  forest::daemon            > Using network :: cannot get name
 2022-07-22T12:21:28.002Z INFO  genesis                   > Importing chain from snapshot
 2022-07-22T12:21:28.004Z INFO  genesis                   > Reading file...
 ...
 2022-07-22T13:02:46.405Z INFO  forest::daemon         > JSON-RPC endpoint started at 127.0.0.1:1234
 2022-07-22T13:02:46.406Z INFO  rpc                    > Ready for RPC connections
```

We can see from the log that:
* it has initialized a keystore at `~/.local/share/forest/keystore.json`
* it printed a JWT token which we will have to attach to each command to be able to talk to the JSON-RPC API

In fact it has already created a wallet which contains the key used to authenticate the JWT token which we can see in the log output.

```console
$ ls ~/.local/share/forest
calibnet  keystore.json  libp2p

$ cat ~/.local/share/forest/keystore.json
{
  "auth-jwt-private": {
    "key_type": 2,
    "private_key": "zVkxTWtojgIe6w+B4GzVLlNCME0RrfUvFYP70MhyWsk="
  }
}
```

Since we need the JWT for each request, let's put it in a variable.

```bash
JWT_TOKEN=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJBbGxvdyI6WyJyZWFkIiwid3JpdGUiLCJzaWduIiwiYWRtaW4iXX0.1zvrh2cLDyJl9K3fXeURy4sLZyEQPwHa7-p26VA_ztg
```

Let's see what happens if we create a new wallet. We don't need to do this, becuase we already have a BLS key we want to put in, but it might help guide us later.

```console
$ forest --token $JWT_TOKEN wallet new bls
2022-07-22T13:03:12.841Z WARN  forest::cli > No configurations found, using defaults.
2022-07-22T13:03:12.846Z ERROR forest::cli > JSON RPC Error: Code: 403 Message: Error code from HTTP Response: 403
```

Hm, we get `Forbidden`. As it turns out, the `--token` option is currently not used, we have to pass the token another way,
using the `FULLNODE_API_INFO` env var, which is expected to be a concatenation of the JWT token and the API multiaddress.

```bash
export FULLNODE_API_INFO=$JWT_TOKEN:/ip4/127.0.0.1/tcp/1234/http
```

Check again:

```console
$ forest wallet new bls
2022-07-22T13:24:45.304Z WARN  forest::cli > No configurations found, using defaults.
f3q64wm4crxbydogt4ls3wgutnf4kb4kbxmrsj5gerg2wggh67fz65igtiqgcfdidnnj5ypetcvb22fhs3opjq

$ cat ~/.local/share/forest/keystore.json
{
  "wallet-f3q64wm4crxbydogt4ls3wgutnf4kb4kbxmrsj5gerg2wggh67fz65igtiqgcfdidnnj5ypetcvb22fhs3opjq": {
    "key_type": 2,
    "private_key": "+WU+/1xpWeR/feV6NnAyUThsTHAal9OHa10I7FrTxjY="
  },
  "default": {
    "key_type": 2,
    "private_key": "+WU+/1xpWeR/feV6NnAyUThsTHAal9OHa10I7FrTxjY="
  },
  "auth-jwt-private": {
    "key_type": 2,
    "private_key": "zVkxTWtojgIe6w+B4GzVLlNCME0RrfUvFYP70MhyWsk="
  }
}
```

It created an address with a key that consists of the `wallet-` prefix and then the public key in hexadecimal format, and has the private key in what looks like Base64.

Okay, now check the command that we really need:

```console
$ forest wallet import --help
forest-wallet-import 0.2.2
Import keys from existing wallet

USAGE:
    forest wallet import [path]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <path>    The path to the private key

```

Promising. It doesn't say, but the path actually needs to point at a hexadecimal encoded JSON of a `KeyInfo` type above, with `key_type` and `private_key` fields. Let's see what the private key file we created earlier actually contains.

```console
$ cat blockchain/consensus/deleg_cns/genesis-files/pre-seal-t01000.key | xxd -r -p
7b2254797065223a22626c73222c22507269766174654b6579223a22326f6f797239694368624d436872554b2b656e393452594b7231466c456a314f613950676c54656d31694d3d227d

$ cat blockchain/consensus/deleg_cns/genesis-files/pre-seal-t01000.key | xxd -r -p
{"Type":"bls","PrivateKey":"2ooyr9iChbMChrUK+en94RYKr1FlEj1Oa9PglTem1iM="}
```

That looks like exactly what we need! The PascalCasing can potentially be a problem.

```console
$ forest wallet import blockchain/consensus/deleg_cns/genesis-files/pre-seal-t01000.key
2022-07-22T13:46:07.237Z WARN  forest::cli > No configurations found, using defaults.
f3xe65dytq7cislbtg3f5clk47bl3mmbl6kurugybsfjzzn7l2tws5fhpvcttcmz4j3zedur7a6zc3snwk67pq

$ cat ~/.local/share/forest/keystore.json
{
  "default": {
    "key_type": 2,
    "private_key": "+WU+/1xpWeR/feV6NnAyUThsTHAal9OHa10I7FrTxjY="
  },
  "wallet-f3xe65dytq7cislbtg3f5clk47bl3mmbl6kurugybsfjzzn7l2tws5fhpvcttcmz4j3zedur7a6zc3snwk67pq": {
    "key_type": 2,
    "private_key": "2ooyr9iChbMChrUK+en94RYKr1FlEj1Oa9PglTem1iM="
  },
  "auth-jwt-private": {
    "key_type": 2,
    "private_key": "zVkxTWtojgIe6w+B4GzVLlNCME0RrfUvFYP70MhyWsk="
  },
  "wallet-f3q64wm4crxbydogt4ls3wgutnf4kb4kbxmrsj5gerg2wggh67fz65igtiqgcfdidnnj5ypetcvb22fhs3opjq": {
    "key_type": 2,
    "private_key": "+WU+/1xpWeR/feV6NnAyUThsTHAal9OHa10I7FrTxjY="
  }
}
```

Happy days. If we look closely at the original `pre-seal-t01000.json` or the `genesis.json` file we can see
that the `Worker` address is `t3xe65dytq7cislbtg3f5clk47bl3mmbl6kurugybsfjzzn7l2tws5fhpvcttcmz4j3zedur7a6zc3snwk67pq`,
which is almost the same as what we have here, save for the `t` prefix versus `f`. `t` is the prefix for testnet,
and `f` is for mainnet, so something's not right there, we won't be able to resolve the worker key to this unless
they match exactly.

By the looks of it, the API does not let us specify whether the key is for mainnet or testnet.

I don't have a better idea than to edit the file and change the prefixes:

```bash
sed -i s/wallet-f/wallet-t/ ~/.local/share/forest/keystore.json
```

## Build Forest with Delegated Consensus

To enable _Delegated Consensus_ instead of the default _Filecoin Consensus_, Forest has to be built with the `deleg_cns` feature, for example:

```bash
cargo clippy --all-targets --features deleg_cns -- -D warnings
cargo build --features deleg_cns --bin forest
```

Now let's try to run a node; it would be the one eligible for mining, since we have the private key in the wallet. We'll point others at different
data directories to they don't have access to it.

What we don't want is for this node to start syncing with the testnet, becuase it won't be able to validate those block (they were created by other miners after all), so network discovery is disabled and the number of peers is set to zero. This won't work if we want to use this node to let
others bootstrap from it, but maybe at that point we can point test nodes to each other.

This is the time for us to use our custom `genesis.car` file as well.

```console
$ rm -rf ~/.local/shared/forest/calibnet
$ RUST_BACKTRACE=1 ./target/debug/forest --encrypt-keystore false --chain calibnet --target-peer-count 0 --kademlia false --snapshot blockchain/consensus/deleg_cns/genesis-files/genesis.car
 2022-07-22T14:46:08.334Z WARN  forest::cli > No configurations found, using defaults., fn_name=forest::cli::find_default_config::hce1f9afe18052660
 2022-07-22T14:46:08.335Z INFO  forest::daemon > Starting Forest daemon, version v0.2.2/unstable/2b20acfb, fn_name=forest::daemon::start::{{closure}}::hdbf1518959c979df
 2022-07-22T14:46:08.335Z INFO  forest_libp2p::service > Recovered libp2p keypair from "/home/aakoshh/.local/share/forest/libp2p/keypair", fn_name=forest_libp2p::service::get_keypair::h99ed6c6ce34645e8
 2022-07-22T14:46:08.336Z WARN  forest::daemon         > Warning: Keystore encryption disabled!, fn_name=forest::daemon::start::{{closure}}::hdbf1518959c979df
 2022-07-22T14:46:08.337Z INFO  metrics                > Prometheus server started at 0.0.0.0:6116, fn_name=metrics::init_prometheus::{{closure}}::h5e34e0d5569c88b6
 2022-07-22T14:46:08.337Z INFO  forest::daemon         > Admin token: eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJBbGxvdyI6WyJyZWFkIiwid3JpdGUiLCJzaWduIiwiYWRtaW4iXX0.1zvrh2cLDyJl9K3fXeURy4sLZyEQPwHa7-p26VA_ztg, fn_name=forest::daemon::start::{{closure}}::hdbf1518959c979df
 2022-07-22T14:46:08.486Z INFO  genesis                > Initialized genesis: BlockHeader: Cid(bafy2bzacecd372hwzypyklyxqdib3zava5av77yvi2zhto4nf3f43qlpx6qha), fn_name=genesis::read_genesis_header::{{closure}}::he72a53f4e30d5477
 2022-07-22T14:46:08.534Z INFO  forest::daemon         > Using network :: cannot get name, fn_name=forest::daemon::start::{{closure}}::hdbf1518959c979df
 2022-07-22T14:46:08.539Z WARN  forest_libp2p::service > Failed to bootstrap with Kademlia: Kademlia is not activated, fn_name=forest_libp2p::service::Libp2pService<DB>::new::he277f49f745948a6
thread 'main' panicked at 'called `Result::unwrap()` on an `Err` value: Unknown miner actor code bafk2bzacebze3elvppssc6v5457ukszzy6ndrg6xgaojfsqfbbtg3xfwo4rbs

Stack backtrace:
   0: anyhow::error::<impl core::convert::From<E> for anyhow::Error>::from
   1: <core::result::Result<T,F> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible,E>>>::from_residual
   2: deleg_cns::consensus::DelegatedConsensus::proposer::{{closure}}

   ...
```

Alas, it looks like Forest is not happy with the Genesis file. It could be becuase it doesn't support that format any more. Indeed, the [error](https://github.com/ChainSafe/forest/blob/d4c25e53c02bc8e319d73c47e8f6bc16a714bdec/vm/actor_interface/src/builtin/miner/mod.rs#L50-L56) indicates that Forest doesn't expect anything less than V8 of the actor state.

It looks like all our efforst have been in vain. We have to be able to generate a genesis file that Forest can read directly to be able to spin up a custom network.
