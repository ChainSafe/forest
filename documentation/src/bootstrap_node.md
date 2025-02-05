# Bootstrap node

⚠️ **The Forest bootstrap node connectivity turned out to be below expectations,
as were the hardware requirements. As such, it's better to hold off hosting
Forest as a bootstrap node until issue
[#4346](https://github.com/ChainSafe/forest/issues/4346) is resolved.**

## Introduction

A bootstrap node is the first node a new node contacts when it joins the
network. It is responsible for providing the new node with a list of other nodes
in the network, which the new node can then contact to join the network. Every
Forest node has a list of bootstrap nodes that it can contact to join the
network. This list is hardcoded into the node but can be modified by the user
via the configuration file.

## Forest as a bootstrap node

Every Forest node can act as a bootstrap node. That said, running a `stateless`
node as a bootstrap node is recommended to lower the hardware requirements. A
`stateless` node does not store the network's state or participate in the
consensus process. It only serves as a gateway for new nodes to join the
network.

Stateless node characteristics:

- it connects to the P2P swarm but does not store the state of the network,
- it does not sync the chain,
- it does not validate the chain,
- `Hello` requests' heaviest tipset is the genesis tipset (unless the node was
  initialized with a snapshot),
- `ChainExchange` responses are `PartialResponses`.

## Running a Forest node as a bootstrap node

To run Forest with the stateless mode enabled, you must set the `--stateless`
flag when starting the node. For example:

```bash
# Mainnet
forest --stateless

# Calibnet
forest --stateless --chain calibnet
```

The default peer count is likely too small for a bootstrap node. You can set the
`--target-peer-count <number>` flag to increase the number of peers. For
example:

```bash
forest --stateless --target-peer-count 10000
```

## Hardware requirements

The stateless node has lower hardware requirements than a full node. The exact
requirements depend on the number of allowed peers. For 10'000 peers, 512 MiB of
RAM and 1 vCPU should be sufficient.

## Converting Lotus node into Forest node (and back)

You can use the `forest-tool shed` commands to convert a Lotus node into a
Forest node without losing the peer identity.

First, the data of both the Lotus and Forest nodes must be backed up. By
default, relevant keys in Lotus are in `~/.lotus/keystore` and in Forest in
`~/.local/share/forest/libp2p/`.

### Lotus to Forest

You need to convert the Lotus key into a Forest key. In the `~/.lotus/keystore`
directory, identify the file with the `libp2p-host` type. For example:

```json
{ "Type": "libp2p-host", "PrivateKey": "<KEY>" }
```

Write the `PrivateKey` value to a file, for example `lotus_key`. Then, run the
following command:

```bash
forest-tool shed key-pair-from-private-key $(cat lotus_key) | base64 -d > keypair
```

Now you can move the `keypair` file to the `~/.local/share/forest/libp2p/`
directory. Done!

### Forest to Lotus

First, convert the keypair file used by Forest into a private key used by Lotus:

```bash
forest-tool shed private-key-from-key-pair > lotus_key
```

Then, copy the content to the relevant file's (one with the type `libp2p-host`
in `~/.lotus/keystore/`) `PrivateKey` value. Done!

## Additional resources

- [DHT Bootstrap nodes](https://blog.ipfs.tech/2023-rust-libp2p-based-ipfs-bootstrap-node/#ipfs-public-dht-bootstrap-nodes)
