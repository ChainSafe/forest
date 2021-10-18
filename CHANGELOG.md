## Forest v0.1.0-alpha (2021-10-19)

ChainSafe System's first alpha release of the _Forest_ Filecoin Rust protocol implementation.
* It synchornizes and verifies the latest Filecoin main network and is able to query the latest state.
* It implements all core systems of the Filecoin protocol specification exposed through a command-line interface.
* The set of functionalities for this first alpha-release include: Message Pool, State Manager, Chain and Wallet CLI functionality, Prometheus Metrics, and a JSON-RPC Server.

The Forest mono-repository contains ten main components (in logical order):
* `forest`: the command-line interface and daemon (1 crate/workspace)
* `node`: the networking stack and storage (7 crates)
* `blockchain`: the chain structure and synchronization (6 crates)
* `vm`: state transition and actors, messages, addresses (9 crates)
* `key_management`: Filecoin account management (1 crate)
* `crypto`: cryptographic functions, signatures, and verification (1 crate)
* `encoding`: serialization library for encoding and decoding (1 crate)
* `ipld`: the IPLD model for content-addressable data (9 crates)
* `types`: the forest types (2 crates)
* `utils`: the forest toolbox (12 crates)

All initial change sets:
* _@TODO_
