# Forest-produced devnet (Curio seals and mines)

Same chain topology as [`../devnet-forest-miner`](../devnet-forest-miner), with Curio in place of Lotus Miner:

|                 | this stack             |
| --------------- | ---------------------- |
| Block producer  | **Forest** + **Curio** |
| Validating node | Lotus                  |
| HarmonyDB       | Yugabyte               |

`NETWORK_NAME=localnet` — Curio rejects names that do not start with `local` or `test`. Upgrade heights are shared via [`../devnet/upgrade_heights.env`](../devnet/upgrade_heights.env) (a plain `2k` Curio binary assumes the wrong genesis schedule).

`setup.sh` builds `forest/curio-devnet:<CURIO_COMMIT>` once from that release tag (about 15 minutes) and reuses it afterward.

```shell
mise run test:devnet:miner:curio
```
