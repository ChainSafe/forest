# Forest-produced devnet (Lotus validates)

Mirrors [`../devnet`](../devnet) with Lotus and Forest roles swapped:

|                 | `../devnet`              | this devnet              |
| --------------- | ------------------------ | ------------------------ |
| Block producer  | Lotus node + Lotus Miner | **Forest** + Lotus Miner |
| Validating node | Forest                   | **Lotus**                |

Lotus Miner produces blocks entirely through Forest's RPC (`MinerGetBaseInfo` ->
`MinerCreateBlock` -> `SyncSubmitBlock`); Forest gossips them and the Lotus node
validates and follows.

## Integration tests

```shell
mise run test:devnet:forest-miner
```
