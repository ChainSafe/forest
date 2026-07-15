# forest on coreos + k3s

> UNOFFICIAL: community contribution from Arkadiy Kukarkin (Internet Archive, FF)

single-node Forest full node on Fedora CoreOS. k3s auto-deploys it, Caddy fronts
the RPC with TLS. boot one ignition file and walk away -- Forest pulls its own
snapshot, Caddy gets its own cert.

## use

```bash
cp .env.example .env && $EDITOR .env    # ssh key, domain, networks
make                                     # -> out/config.ign
```

install FCOS with that ignition:
- generic FCOS: `coreos-installer install /dev/nvme0n1 --ignition-file out/config.ign`
- Hetzner rescue: `scp out/config.ign root@<rescue-ip>:` then flash the FCOS image
  and pass the ignition (see coreos-installer docs)

then wait. it's up when:
```bash
curl https://<domain>/rpc/v1 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","method":"Filecoin.ChainHead","params":[],"id":1}'
ssh core@<ip> 'k3s kubectl -n forest-mainnet exec deploy/forest -- forest-cli sync status'
```

## networks

`NETWORKS` in `.env`, space-separated (`mainnet`, `calibnet`). first = primary,
served at `/rpc/v1`; others at `/<network>/rpc/v1`. mainnet ~500G, calibnet ~100G,
both need ~800G+.

add one to a live box without rebuilding it:
```bash
scp out/manifests/forest-stack.yaml core@<ip>:
ssh core@<ip> 'sudo k3s kubectl apply -f forest-stack.yaml'
```

## JWT gate (optional)

`AUTH=jwt`, `make`, then once the box is up:
```bash
make jwt HOST=core@<ip>              # builds the caddy-jwt plugin on the box, swaps caddy
make token HOST=core@<ip> SUB=ci     # mint a token
```
present it as `Authorization: Bearer <t>` or `?access_token=<t>`. needs a real
domain (the gate rides on TLS).

## requirements

- `butane` on PATH, or `podman` (render falls back to the butane container)
- a domain pointing at the box for a real cert; empty `DOMAIN` -> self-signed
- box, per forest's own hardware requirements for an RPC node with ~2mo retention:
  6-core / 16 GiB low traffic, 8-core / 32 GiB high traffic, NVMe.
  disk rule of thumb: 200 GiB + 5 GiB per day of retention (500 GiB ~= 60 days).
  `FOREST_*` in `.env` default to the low-traffic profile.

## notes

- FCOS auto-updates are off (no reboots mid-sync); flip in `config.bu.in` to re-enable.
- single-disk local-path, no redundancy -- bring your own for prod.
- RPC is lotus-compatible and serves both Filecoin and eth JSON-RPC on `/rpc/v1`.
