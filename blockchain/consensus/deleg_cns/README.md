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

Alternatively we can build `lotus-seed` directly like so:

```bash
GOFLAGS=-tags=2k make lotus-seed
```

Notice the `2k` tag: it activates the [params_2k](https://github.com/filecoin-project/lotus/blob/v1.17.0-rc3/build/params_2k.go#L2) settings, which results in the actor bundles for `devnet` to be written into the `genesis.car` file. Forest [only accepts](https://github.com/ChainSafe/forest/blob/d4c25e53c02bc8e319d73c47e8f6bc16a714bdec/vm/actor_interface/src/builtin/miner/mod.rs#L27-L35) the ones for `mainnet` or `calibnet`, but I did not manage to make a `genesis.car` file for those (built with `make lotus-seed` and `make calibnet`). The actor CIDs in different bundles can be seen [here](https://github.com/filecoin-project/lotus/blob/v1.17.0-rc3/build/builtin_actors_gen.go). For this experiment I will just have to whitelist the `devnet` ones in Forest.


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
  "NetworkVersion": 0,
  "Accounts": [
    {
      "Type": "account",
      "Balance": "50000000000000000000000000",
      "Meta": {
        "Owner": "t3vvrnnenu67ux2nk7tdiop53lvn3ruayaa4n5bcimewef76ocgchx2ivd2k7hoaxmzme7dfc53vi7bofzz2eq"
      }
    }
  ],
  "Miners": [
    {
      "ID": "t01000",
      "Owner": "t3vvrnnenu67ux2nk7tdiop53lvn3ruayaa4n5bcimewef76ocgchx2ivd2k7hoaxmzme7dfc53vi7bofzz2eq",
      "Worker": "t3vvrnnenu67ux2nk7tdiop53lvn3ruayaa4n5bcimewef76ocgchx2ivd2k7hoaxmzme7dfc53vi7bofzz2eq",
      "PeerId": "12D3KooWEGdCsa4DAP8HG4r33ymxXFcXtui192zJnY48Hjzr4KKF",
      "MarketBalance": "0",
      "PowerBalance": "0",
      "SectorSize": 2048,
      "Sectors": [
        {
          "CommR": {
            "/": "bagboea4b5abcbl3oevwwjgxgtns4rg7erxp2vq33pxvszp6t534jhtxhc2qvtwdr"
          },
          "CommD": {
            "/": "baga6ea4seaqe4lcia4en7czkklnjktcmby2pp7gn4fltztgkh6mel4yzhnrqyiy"
          },
          "SectorID": 0,
          "Deal": {
            "PieceCID": {
              "/": "baga6ea4seaqe4lcia4en7czkklnjktcmby2pp7gn4fltztgkh6mel4yzhnrqyiy"
            },
            "PieceSize": 2048,
            "VerifiedDeal": false,
            "Client": "t3vvrnnenu67ux2nk7tdiop53lvn3ruayaa4n5bcimewef76ocgchx2ivd2k7hoaxmzme7dfc53vi7bofzz2eq",
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
            "PrivateKey": "l1k51pFipvshUSmAHcjoYXiDZX+Z7Q1Jcd/bk4EILgg=",
            "PublicKey": "rWLWkbT36X01X5jQ5/drq3caAwAHG9CJDCWIX/nCMI99IqPSvncC7MsJ8ZRd3VHw",
            "Address": "t3vvrnnenu67ux2nk7tdiop53lvn3ruayaa4n5bcimewef76ocgchx2ivd2k7hoaxmzme7dfc53vi7bofzz2eq"
          },
          "ProofType": 0
        }
      ]
    }
  ],
  "NetworkName": "localnet-514af5d5-6517-4e40-b4da-258d3200b9f3",
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

Note the following part: `"NetworkVersion": 0,`. It has to be version `16` for Forest to handle it, and for the right actor bundles to be inserted into the `genesis.car` file. Let's edit it:


```bash
sed -i "s/\"NetworkVersion\": 0/\"NetworkVersion\": 16/" ./genesis-files/genesis.json
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

## Build Forest with Delegated Consensus

To enable _Delegated Consensus_ instead of the default _Filecoin Consensus_, Forest has to be built with the `deleg_cns` feature, for example:

```bash
cargo clippy --all-targets --features deleg_cns -- -D warnings
cargo build --release --features deleg_cns --bin forest
```

We better build it now instead of doing it with `make build`, because we will need to run the wallet commands, and they need to connect to a
running node. With Filecoin built for the default delegated consensus, we'd have to import a [snapshot](https://fra1.digitaloceanspaces.com/forest-snapshots/calibnet/calibnet-2022-07-01.car) first, which takes a long time.

Normally it would be possible to run `make install` to make it available globally, but for this exercise we can just run it from the build artifacts; make can make an alias so the commands look identical.

```bash
alias forest=target/release/forest
```

Note that we're using `--release` mode. This takes longer to build, but without it, loading the Wasm bundles takes forever.

## Run a node without proposing blocks

At this point our wallet doesn't contain any private keys, so if we run a node with _Delegated Consensus_ it should be a pure follower, without trying to produce any blocks.

We want the network to start from the latest version, because that's the only version supported by Forest. The [proposer-config.toml](blockchain/consensus/deleg_cns/configs/proposer-config.toml) file tells Forest to use V16 for any epoch higher than -1. It also contains all the other heights that it looks for by name.

We also don't want to connect to any other nodes at the moment, so there are no bootstrap nodes set in the config file.

This is the time for us to use our custom `genesis.car` file as well.

```bash
RUST_BACKTRACE=1 forest --encrypt-keystore false --target-peer-count 1 \
  --genesis blockchain/consensus/deleg_cns/genesis-files/genesis.car \
  --config blockchain/consensus/deleg_cns/configs/proposer-config.toml
```

We have to look at the output to see what API tokens we'll have to use with the wallet:

```console
$ RUST_BACKTRACE=1 forest --encrypt-keystore false --target-peer-count 1 \
        --genesis blockchain/consensus/deleg_cns/genesis-files/genesis.car \
        --config blockchain/consensus/deleg_cns/configs/proposer-config.toml
 2022-08-02T19:19:08.855Z INFO  forest::daemon > Starting Forest daemon, version v0.2.2/unstable/961ac230, fn_name=forest::daemon::start::{{closure}}::heec41d53b90815ef
 2022-08-02T19:19:08.855Z INFO  forest_libp2p::service > Networking keystore not found!, fn_name=forest_libp2p::service::get_keypair::ha8162726e04c5572
 2022-08-02T19:19:08.856Z INFO  utils                  > Permissions set to 0600 on File { fd: 6, path: "/home/aakoshh/.local/share/forest/libp2p/keypair", read: false, write: true }, fn_name=utils::set_user_perm::h4bdd96d06f9f33ec
 2022-08-02T19:19:08.857Z WARN  forest::daemon         > Warning: Keystore encryption disabled!, fn_name=forest::daemon::start::{{closure}}::heec41d53b90815ef
 2022-08-02T19:19:08.857Z WARN  key_management::keystore > Keystore does not exist, initializing new keystore at: "/home/aakoshh/.local/share/forest/keystore.json", fn_name=key_management::keystore::KeyStore::new::h0ff0fb32ca2f6c01
 2022-08-02T19:19:08.857Z INFO  utils                    > Permissions set to 0600 on File { fd: 6, path: "/home/aakoshh/.local/share/forest/keystore.json", read: false, write: true }, fn_name=utils::set_user_perm::h4bdd96d06f9f33ec
 2022-08-02T19:19:08.858Z INFO  forest::daemon           > Prometheus server started at 127.0.0.1:6116, fn_name=forest::daemon::start::{{closure}}::heec41d53b90815ef
 2022-08-02T19:19:08.858Z INFO  forest::daemon           > Admin token: eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJBbGxvdyI6WyJyZWFkIiwid3JpdGUiLCJzaWduIiwiYWRtaW4iXX0.LmvzLxFmsVr7etpQjeCGKf5UoEffhuziKKHdr5ascjE, fn_name=forest::daemon::start::{{closure}}::heec41d53b90815ef
 2022-08-02T19:19:08.921Z WARN  chain::store::chain_store > No previous chain state found, fn_name=chain::store::chain_store::ChainStore<DB>::load_heaviest_tipset::{{closure}}::ha8c2a7c73282a0d5
 2022-08-02T19:19:08.974Z INFO  genesis                   > Initialized genesis: BlockHeader: Cid(bafy2bzacec32hemorofji6x4bymnisdbre5qxcteuoqz5bulp76xn5mtc62y2), fn_name=genesis::read_genesis_header::{{closure}}::hbed3f12d958e0eb7
 2022-08-02T19:19:09.039Z INFO  forest::daemon            > Using network :: devnet, fn_name=forest::daemon::start::{{closure}}::heec41d53b90815ef
 2022-08-02T19:19:09.052Z WARN  forest_libp2p::service    > Failed to bootstrap with Kademlia: Kademlia is not activated, fn_name=forest_libp2p::service::Libp2pService<DB>::new::hcaf01e98596a90f2
 2022-08-02T19:19:09.061Z INFO  forest::daemon            > JSON-RPC endpoint started at 127.0.0.1:1234, fn_name=forest::daemon::start::{{closure}}::{{closure}}::h52afcfa467fed7f5
 2022-08-02T19:19:09.063Z INFO  rpc                       > Ready for RPC connections, fn_name=rpc::start_rpc::{{closure}}::h3dabc34937b47105
```

We can see from the log that:
* it has initialized a keystore at `~/.local/share/forest/keystore.json`
* it printed a JWT token which we will have to attach to each command to be able to talk to the JSON-RPC API

In fact it has already created a wallet which contains the key used to authenticate the JWT token which we can see in the log output.

```console
$ ls ~/.local/share/forest
devnet  keystore.json  libp2p

$ cat ~/.local/share/forest/keystore.json
{
  "auth-jwt-private": {
    "key_type": 2,
    "private_key": "g8YSmsplwsHOr612EwNtIUr+Ftofl4CSIH/epbGgEw4="
  }
}
```

Note that it has created the `devnet` directory. Forest only accepts networks it recognises, so the `proposer-config.toml` has `devnet` as the network name, but this has nothing to do with any actual official devnet.

## Add the private key to a wallet

One of the Forest processes we start will need access to the private key, and normally it looks for it in the wallet. To prepare a wallet, we can use Forest itself because it seems to have all the necessary [commands](https://github.com/ChainSafe/forest/blob/v0.3.0/forest/src/cli/wallet_cmd.rs).

We will need to attach the the JWT to each request, let's put it in a variable.

```bash
JWT_TOKEN=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJBbGxvdyI6WyJyZWFkIiwid3JpdGUiLCJzaWduIiwiYWRtaW4iXSwiZXhwIjoxNjczMjEwMTkzfQ.xxhmqtG9O3XNTIrOEB2_TWnVkq0JkqzRdw63BdosV0c
```

Let's see what happens if we create a new wallet. We don't need to do this, becuase we already have a BLS key we want to put in, but it might help guide us later.

```console
$ forest-cli --token $JWT_TOKEN wallet new bls
 2022-10-27T07:45:44.172Z DEBUG forest_rpc_client > Using JSON-RPC v2 HTTP URL: http://127.0.0.1:1234/rpc/v0
f3te7mpfmtbgp5woqlnojungdxyvbkzme76x7xof6w7lpahrd5it53ljz2eyykdm2gkzmi3lofv22usbzn3uia
```

we can also pass the token another way,
using the `FULLNODE_API_INFO` env var, which is expected to be a concatenation of the JWT token and the API multiaddress.

```bash
export FULLNODE_API_INFO=$JWT_TOKEN:/ip4/127.0.0.1/tcp/1234/http
```

```console
$ forest-cli wallet new bls
 2022-08-02T19:27:15.950Z WARN  forest::cli > No configurations found, using defaults.
f3taxhvwkyogdqb3rxrt7e5xrdsvpibxdsmadqwvlcq6pjplu6wwt4bejnukqqysnxpqkapwhbnjz5nejh3dza


$ cat ~/.local/share/forest/keystore.json
{
  "wallet-f3qljpcw4zxavl6dka2b6j6l36cy75g5pyt44oiwixm2f2jkttti7apdi4mhrof5qnmxhzun6kpuqqvrq75zqa": {
    "key_type": 2,
    "private_key": "aTvS8uBJ9ujoEsAcsEflp4hXF7vi23ZqzWbDlVBSSko="
  },
  "default": {
    "key_type": 2,
    "private_key": "aTvS8uBJ9ujoEsAcsEflp4hXF7vi23ZqzWbDlVBSSko="
  },
  "auth-jwt-private": {
    "key_type": 2,
    "private_key": "g8YSmsplwsHOr612EwNtIUr+Ftofl4CSIH/epbGgEw4="
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
$ cat blockchain/consensus/deleg_cns/genesis-files/pre-seal-t01000.key
7b2254797065223a22626c73222c22507269766174654b6579223a224d44796d3236657744664f4a6d752b7749676e4c6855315a386d4c657255364e65664a324468724c3755593d227d

$ cat blockchain/consensus/deleg_cns/genesis-files/pre-seal-t01000.key | xxd -r -p
{"Type":"bls","PrivateKey":"MDym26ewDfOJmu+wIgnLhU1Z8mLerU6NefJ2DhrL7UY="}
```

That looks like exactly what we need! The PascalCasing can potentially be a problem.

```console
$ forest wallet import blockchain/consensus/deleg_cns/genesis-files/pre-seal-t01000.key
 2022-08-02T19:29:46.408Z WARN  forest::cli > No configurations found, using defaults.
f3ugf5w4krrovxc64n5vi62t6qrbs5rm6zpgpmrt7b2ukaxescesbutbgmppn6rlstiihxd4dkrt7viqacuxoq

$ cat ~/.local/share/forest/keystore.json
{
  "wallet-f3ugf5w4krrovxc64n5vi62t6qrbs5rm6zpgpmrt7b2ukaxescesbutbgmppn6rlstiihxd4dkrt7viqacuxoq": {
    "key_type": 2,
    "private_key": "MDym26ewDfOJmu+wIgnLhU1Z8mLerU6NefJ2DhrL7UY="
  },
  "default": {
    "key_type": 2,
    "private_key": "aTvS8uBJ9ujoEsAcsEflp4hXF7vi23ZqzWbDlVBSSko="
  },
  "auth-jwt-private": {
    "key_type": 2,
    "private_key": "g8YSmsplwsHOr612EwNtIUr+Ftofl4CSIH/epbGgEw4="
  },
  "wallet-f3qljpcw4zxavl6dka2b6j6l36cy75g5pyt44oiwixm2f2jkttti7apdi4mhrof5qnmxhzun6kpuqqvrq75zqa": {
    "key_type": 2,
    "private_key": "aTvS8uBJ9ujoEsAcsEflp4hXF7vi23ZqzWbDlVBSSko="
  }
}
```

Happy days. If we look closely at the original `pre-seal-t01000.json` or the `genesis.json` file we can see
that the `Worker` address is `t3ugf5w4krrovxc64n5vi62t6qrbs5rm6zpgpmrt7b2ukaxescesbutbgmppn6rlstiihxd4dkrt7viqacuxoq`,
which is almost the same as what we have here, save for the `t` prefix versus `f`. `t` is the prefix for testnet,
and `f` is for mainnet, so something's not right there, we won't be able to resolve the worker key to this unless
they match exactly.

By the looks of it, the API does not let us specify whether the key is for mainnet or testnet.

I don't have a better idea than to edit the file and change the prefixes:

```bash
sed -i s/wallet-f/wallet-t/ ~/.local/share/forest/keystore.json
```

## Run the block producer

Let's try to run a node again; it should be eligible for mining, since we have the private key in the wallet. We'll point others at different
data directories to they don't have access to it.


```console
$ RUST_BACKTRACE=1 forest --encrypt-keystore false --target-peer-count 1 \
  --genesis blockchain/consensus/deleg_cns/genesis-files/genesis.car \
  --config blockchain/consensus/deleg_cns/configs/proposer-config.toml

 ...
 2022-08-02T19:37:54.296Z INFO  forest::daemon         > Starting the delegated consensus proposer..., fn_name=forest::daemon::start::{{closure}}::heec41d53b90815ef
 ...
 2022-08-02T19:38:24.311Z INFO  deleg_cns::proposer    > Proposed block bafy2bzacecxbvobik6rdrl53ba2x2czkcewxi7pw3skcabmxgwonhyes7k2gs with 0 messages, fn_name=<deleg_cns::proposer::DelegatedProposer as chain_sync::consensus::Proposer>::run::{{closure}}::he99e8348f9fb9561
 2022-08-02T19:38:24.314Z WARN  forest_libp2p::service > Failed to send gossipsub message: InsufficientPeers, fn_name=forest_libp2p::service::Libp2pService<DB>::run::{{closure}}::hcd55a2962486211d
 2022-08-02T19:38:54.303Z INFO  deleg_cns::proposer    > Proposed block bafy2bzacecxbvobik6rdrl53ba2x2czkcewxi7pw3skcabmxgwonhyes7k2gs with 0 messages, fn_name=<deleg_cns::proposer::DelegatedProposer as chain_sync::consensus::Proposer>::run::{{closure}}::he99e8348f9fb9561
 2022-08-02T19:38:54.304Z WARN  forest_libp2p::service > Failed to send gossipsub message: InsufficientPeers, fn_name=forest_libp2p::service::Libp2pService<DB>::run::{{closure}}::hcd55a2962486211d
 ...
```

Grand! The node is trying to propose blocks every 30 seconds, but it complains about not having peers.

### Inconsistent work addresses

If we look at the log we can see that it actually proposed the same block twice, because it was unable to add them to the chain, so it always built on the same parent, with the same timestamp.

There were multiple reasons for this:
1. The node was waiting for P2P messages to figure out if it's in sync with the network, even though it's alone. This has been disabled by setting the `tipset_sample_size` to `0`.
2. Validation failed with an error saying the miner wasn't eligible to mine.

Upon inspection it turned out that despite the `Worker` starting with `t` in the `genesis.json` file, the address that has been written to the actually starts with `f`, so when we look up the address of `t01000` we get back `f3ugf5w4krrovxc64n5vi62t6qrbs5rm6zpgpmrt7b2ukaxescesbutbgmppn6rlstiihxd4dkrt7viqacuxoq`.

Furthermore, for some reason Forest only creates `Address` from an ID with the default `mainnet` network, so it stripped away the `t` and added back `f`, like it did for the wallet.

For now a fix has been added so that the proposer looks up its own address to convert it into the `f` prefixed version, before looking up the key in the wallet, and doing any comparisons. This way the bug cancels itself out.

It also means we have to undo any changes to the wallet keystore:

```bash
sed -i s/wallet-t/wallet-f/ ~/.local/share/forest/keystore.json
```

### Setup a network

Next, we'll have to set up a network of nodes, to see that not only can we produce blocks, but also validate them on other nodes.

To do so we'll have to set the `bootstrap_peers` in the config file to contain the multiaddress of our proposer. It should look like `/ip4/127.0.0.1/tcp/1234/p2p/<peer-id-multihash>`.

To find out what the peer ID is, a log has been added to the output:

```
...
2022-08-02T20:04:18.271Z INFO  forest::daemon         > PeerId: 12D3KooWNEmsCbySkVBbZhX12kxommT3uFcNnoBS4Z8PN79r4aKi, fn_name=forest::daemon::start::{{closure}}::heec41d53b90815ef
...
```

So let's edit the `configs/delegator-config.toml` like so:

```toml
[network]
bootstrap_peers = [
  "/ip4/127.0.0.1/tcp/2340/p2p/12D3KooWNEmsCbySkVBbZhX12kxommT3uFcNnoBS4Z8PN79r4aKi"
]
```

The port `2340` was set as a `listening_multiaddr` in `proposer-config.toml` so we know what to expect, because the node doesn't report it if we use `0`, except with `RUST_LOG=debug` on. Or one can use the `forest net listen` command to list listening addresses once the node is running.

And start it in another terminal window:

```bash
RUST_BACKTRACE=1 forest --encrypt-keystore false --target-peer-count 1 \
  --genesis blockchain/consensus/deleg_cns/genesis-files/genesis.car \
  --config blockchain/consensus/deleg_cns/configs/delegator-config.toml
```
