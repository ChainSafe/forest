# 🌲 Forest 
![](https://github.com/ChainSafe/forest/workflows/Rust%20CI/badge.svg?branch=master)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Discord](https://img.shields.io/discord/593655374469660673.svg?label=Discord&logo=discord)](https://discord.gg/Q6A3YA2)
[![](https://img.shields.io/twitter/follow/espadrine.svg?label=Follow&style=social)](https://twitter.com/chainsafeth)


Forest is an implementation of [Filecoin](https://filecoin.io/) written in Rust. The implementation will take a modular approach to building a full Filecoin node in two parts — (i) building Filecoin’s security critical systems in Rust from the [Filecoin Protocol Specification](https://filecoin-project.github.io/specs/), specifically the virtual machine, blockchain, and node system, and (ii) integrating functional components for storage mining and storage & retrieval markets to compose a fully functional Filecoin node implementation.

❗**Current development should be considered a work in progress.**

Our crates:

| crate | description |
|-|-|
| `blockchain` | chain structures and syncing functionality |
| `crypto` | verification and signature definitions |
| `encoding` | used for encoding and decoding |
| `ipld` | IPLD data model for content-addressable data |
| `node` | networking synchronization and storage |
| `vm` | state transition and actor, message definitions |

## Dependencies
rustc >= 1.40.0

## Usage
```bash
# download ChainSafe Forest code
git clone https://github.com/chainsafe/forest
cd forest

cargo build && ./target/debug/node
```

### Testing
```
cargo test
```

### Documentation
https://chainsafe.github.io/forest/

## Contributing
- Check out our contribution guidelines: [CONTRIBUTING.md](CONTRIBUTING.md)  
- Have questions? Say hi on [Discord](https://discord.gg/Q6A3YA2)!

## License 
Forest is dual licensed under [MIT](https://github.com/ChainSafe/forest/blob/master/LICENSE-MIT) + [Apache 2.0](https://github.com/ChainSafe/forest/blob/master/LICENSE-APACHE).