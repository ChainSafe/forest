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

On this devnet the runner also runs `forest-dev devnet miner-onboarding`:

- `create_miner` — `lotus-miner init` through Forest; Forest and Lotus must agree on creation
  deposit and the new miner's info.
- `pledge_sector` — real 2 KiB CC seal to `Proving`. The chain verifies the seal proof, so wrong
  Forest randomness fails prove-commit. The miner actor locks the pre-commit deposit and initial
  pledge from the miner's unlocked balance. This sector is pledged on the genesis miner, which
  has won every block and keeps 1/4 of each reward unlocked, so a zero deposit or pledge from
  Forest still reaches `Proving`. Sector info must then match on Forest and Lotus.

Refuses `../devnet` (Lotus Miner would talk to Lotus, not Forest).
