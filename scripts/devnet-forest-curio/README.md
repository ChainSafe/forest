# Forest-produced devnet (Curio seals and mines)

Same chain topology as [`../devnet-forest-miner`](../devnet-forest-miner), with Curio in place of Lotus Miner:

|                 | this stack             |
| --------------- | ---------------------- |
| Block producer  | **Forest** + **Curio** |
| Validating node | Lotus                  |
| HarmonyDB       | Yugabyte               |

`NETWORK_NAME=localnet` because Curio rejects a chain node whose name does not start with `local` or `test`. The Curio container gets the same `LOTUS_*` upgrade-height variables as Lotus; a `2k` Curio binary otherwise assumes a different genesis schedule.

`setup.sh` builds `forest/curio-devnet:<CURIO_COMMIT>` once (about 15 minutes) and reuses it afterward.

```shell
mise run test:devnet:miner:curio
```
