#!/usr/bin/env bash
# render out/config.ign + out/manifests/forest-stack.yaml from .env
set -euo pipefail
cd "$(dirname "$0")/.."

[ -f .env ] || { echo "no .env -- cp .env.example .env and edit"; exit 1; }
set -a; . ./.env; set +a

: "${SSH_PUBKEY:?set SSH_PUBKEY in .env}"
NETWORKS="${NETWORKS:-mainnet}"
AUTH="${AUTH:-public}"
FOREST_IMAGE="${FOREST_IMAGE:-ghcr.io/chainsafe/forest:v0.33.7}"
DOMAIN="${DOMAIN:-}"
ACME_EMAIL="${ACME_EMAIL:-}"
PRIMARY="${NETWORKS%% *}"

# defaults track ChainSafe's "RPC node, 2mo retention, low traffic" profile
export FOREST_CPU_REQUEST="${FOREST_CPU_REQUEST:-2}"
export FOREST_CPU_LIMIT="${FOREST_CPU_LIMIT:-6}"
export FOREST_MEM_REQUEST="${FOREST_MEM_REQUEST:-8Gi}"
export FOREST_MEM_LIMIT="${FOREST_MEM_LIMIT:-16Gi}"

out=out; rm -rf "$out"; mkdir -p "$out/manifests"

net_disk() {
  case "$1" in
    mainnet) echo "${MAINNET_DISK:-500Gi}";;
    calibnet) echo "${CALIBNET_DISK:-100Gi}";;
    *) echo "${DEFAULT_DISK:-200Gi}";;
  esac
}

# Caddyfile for the given mode (public|jwt); routing scheme: primary at root
# and at /<primary>/, additional networks at /<net>/
caddyfile() {
  local mode="$1" net
  if [ "$mode" = jwt ] || [ -n "$ACME_EMAIL" ]; then
    printf '{\n'
    [ -n "$ACME_EMAIL" ] && printf '  email %s\n' "$ACME_EMAIL"
    [ "$mode" = jwt ] && printf '  order jwtauth before basicauth\n'
    printf '}\n'
  fi
  # with no domain there is no name to issue for, so caddy's internal CA needs
  # on_demand or it serves no cert at all and every handshake fails
  if [ -n "$DOMAIN" ]; then
    printf '%s {\n' "$DOMAIN"
  else
    printf ':443 {\n  tls internal {\n    on_demand\n  }\n'
  fi
  if [ "$mode" = jwt ]; then
    printf '  jwtauth {\n    sign_key {env.JWT_SIGN_KEY}\n    sign_alg HS256\n    from_header Authorization\n    from_query access_token\n  }\n'
  fi
  for net in $NETWORKS; do
    printf '  handle_path /%s/* {\n    reverse_proxy forest-rpc.forest-%s.svc.cluster.local:2345 {\n      header_up -Authorization\n    }\n  }\n' "$net" "$net"
  done
  printf '  handle {\n    reverse_proxy forest-rpc.forest-%s.svc.cluster.local:2345 {\n      header_up -Authorization\n    }\n  }\n}\n' "$PRIMARY"
}

# caddy manifest; $1 mode, $2 image
caddy_manifest() {
  local mode="$1" image="$2"
  cat <<EOF
apiVersion: v1
kind: Namespace
metadata: { name: forest-edge }
---
apiVersion: v1
kind: ConfigMap
metadata: { name: caddy, namespace: forest-edge }
data:
  Caddyfile: |
$(caddyfile "$mode" | sed 's/^/    /')
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata: { name: caddy-data, namespace: forest-edge }
spec:
  accessModes: ["ReadWriteOnce"]
  storageClassName: local-path
  resources: { requests: { storage: 1Gi } }
EOF
  if [ "$mode" = jwt ]; then cat <<EOF
---
apiVersion: v1
kind: Secret
metadata: { name: caddy-jwt, namespace: forest-edge }
type: Opaque
stringData: { sign_key: "${SIGN_KEY}" }
EOF
  fi
  cat <<EOF
---
apiVersion: apps/v1
kind: Deployment
metadata: { name: caddy, namespace: forest-edge }
spec:
  replicas: 1
  strategy: { type: Recreate }
  selector: { matchLabels: { app: caddy } }
  template:
    metadata: { labels: { app: caddy } }
    spec:
      hostNetwork: true
      dnsPolicy: ClusterFirstWithHostNet
      containers:
        - name: caddy
          image: ${image}
          command: ["caddy"]
          args: ["run", "--config", "/etc/caddy/Caddyfile", "--adapter", "caddyfile"]
          env:
            - { name: XDG_DATA_HOME, value: /data }
            - { name: XDG_CONFIG_HOME, value: /data }
EOF
  [ "$mode" = jwt ] && printf '            - { name: JWT_SIGN_KEY, valueFrom: { secretKeyRef: { name: caddy-jwt, key: sign_key } } }\n'
  cat <<EOF
          ports:
            - { containerPort: 80, hostPort: 80, name: http }
            - { containerPort: 443, hostPort: 443, name: https }
          resources:
            requests: { cpu: "50m", memory: "64Mi" }
            limits: { cpu: "1", memory: "256Mi" }
          volumeMounts:
            - { mountPath: /etc/caddy, name: config, readOnly: true }
            - { mountPath: /data, name: data }
      volumes:
        - { name: config, configMap: { name: caddy } }
        - { name: data, persistentVolumeClaim: { claimName: caddy-data } }
EOF
}

# forest nodes + public caddy -> the k3s auto-deploy manifest
stack="$out/manifests/forest-stack.yaml"; : > "$stack"
for net in $NETWORKS; do
  NET="$net" DISK="$(net_disk "$net")" IMAGE="$FOREST_IMAGE" \
    envsubst '${NET} ${DISK} ${IMAGE} ${FOREST_CPU_REQUEST} ${FOREST_CPU_LIMIT} ${FOREST_MEM_REQUEST} ${FOREST_MEM_LIMIT}' \
    < templates/forest-node.yaml.tmpl >> "$stack"
  echo '---' >> "$stack"
done
caddy_manifest public docker.io/library/caddy:2-alpine >> "$stack"

# ignition
SSH_PUBKEY="$SSH_PUBKEY" envsubst '${SSH_PUBKEY}' < config.bu.in > "$out/config.bu"
if command -v butane >/dev/null; then
  butane --strict --files-dir "$out" "$out/config.bu" > "$out/config.ign"
else
  podman run --rm -i -v "$PWD/$out:/w:z" quay.io/coreos/butane:release \
    --strict --files-dir /w < "$out/config.bu" > "$out/config.ign"
fi
echo "wrote $out/config.ign  (networks: $NETWORKS, primary: $PRIMARY, auth: public)"

# jwt overlay (applied post-boot with `make jwt`)
if [ "$AUTH" = jwt ]; then
  mkdir -p "$out/jwt"
  SIGN_KEY="$(openssl rand -base64 48 | tr -d '\n')"
  printf '%s' "$SIGN_KEY" > "$out/jwt/sign_key.b64"
  cp jwt/Containerfile "$out/jwt/Containerfile"
  caddy_manifest jwt localhost/caddy-jwt:2 > "$out/jwt/caddy-jwt.yaml"
  echo "auth=jwt: overlay in $out/jwt -- after the box is up run: make jwt HOST=core@<ip>"
fi
