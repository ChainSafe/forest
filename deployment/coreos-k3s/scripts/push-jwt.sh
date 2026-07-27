#!/usr/bin/env bash
# build the caddy-jwt plugin image on the box and swap caddy to the gated config
set -euo pipefail
cd "$(dirname "$0")/.."
HOST="$1"
[ -f out/jwt/caddy-jwt.yaml ] || { echo "no out/jwt -- set AUTH=jwt in .env, run make, then retry"; exit 1; }

ssh "$HOST" 'mkdir -p forest-jwt'
scp out/jwt/Containerfile out/jwt/caddy-jwt.yaml "$HOST:forest-jwt/"
ssh "$HOST" 'bash -s' <<'REMOTE'
set -euo pipefail
podman build -t localhost/caddy-jwt:2 forest-jwt/
podman save localhost/caddy-jwt:2 | sudo k3s ctr images import -
sudo k3s kubectl apply -f forest-jwt/caddy-jwt.yaml
sudo k3s kubectl -n forest-edge rollout restart deploy/caddy
sudo k3s kubectl -n forest-edge rollout status deploy/caddy --timeout=120s
REMOTE
echo "jwt gate on. mint tokens: make token HOST=$HOST SUB=<name>"
